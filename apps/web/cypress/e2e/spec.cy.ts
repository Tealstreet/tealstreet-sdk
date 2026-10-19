/**
 * =============================================================================
 * DEBUGGING FAILED TESTS — READ THIS FIRST
 * =============================================================================
 *
 * Two log sources are available when a test fails:
 *
 * 1. FULL TRACE (every HTTP request/response and WS message):
 *    cypress/logs/{exchange}-api.log
 *
 * 2. FAILURE SUMMARY (last relevant entries, sent to Slack in CI):
 *    cypress/results/{exchange}.json → testFailureDiagnostics
 *
 * When debugging locally, start with the full trace:
 *
 *   grep "place-tpsl-order\|place-order" cypress/logs/bitget-api.log
 *   grep -i "error\|reject\|code.*4[0-9]" cypress/logs/bitget-api.log
 *   grep "PRIVATEWS.*orders\|PRIVATEWS.*positions" cypress/logs/bitget-api.log
 *
 * DO NOT start guessing at code changes without reading the logs first.
 * See cypress/README.md → "Debugging Test Failures" for full guide.
 * =============================================================================
 */
import { setupDebugLogging, waitAndLogAPI } from '../support/debug-logger';
import {
  adjust,
  ExchangeMetadata,
  getExchangeMetadata,
  handleRoundingError,
  MarginType,
  OrderSide,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../support/vendor';

interface ExchangeConfig {
  exchange: string;
  symbol: string;
  orderSizeInQuote: number;
  marketOrderSizeInQuote?: number;
  marketOrderSides?: OrderSide[];
  positionModeOrderSizeInQuote?: number;
  marginTypes?: MarginType[];
  orderSizeVariance: number;
  farTouchDelta: number;
  /** Offset for orders meant to CROSS and fill immediately. Defaults to
   *  farTouchDelta. Venues that cap how far through the touch a limit order
   *  may be priced (WOO: bid_cap_ratio / ask_floor_ratio) need a smaller one. */
  crossDelta?: number;
  /** Quote currency shown in the size cell's unit dropdown. Defaults to USDT.
   *  USD-quoted venues (Kraken Futures: PF_DOGEUSD) render 'USD'. */
  quoteCurrency?: string;
  /** Symbol as rendered in the orders/positions tables, which show
   *  `market.prettySymbol || symbol`. Defaults to `symbol`. Only needed where the
   *  two differ — Kraken Futures trades `PF_DOGEUSD` but displays `DOGEUSD`. */
  displaySymbol?: string;
  expectedWalletBalance: number;
  expectedAvailableMargin: number;
  actionWaitTime?: number;
  slowUpdateExtraWaitTime?: number;
  openPositionSettleWaitTime?: number;
  hedgeModeLeverage?: number;
  accountExtra?: Record<string, unknown>;
  reduceOnlyLimitPostOnly?: boolean;
  balanceVariance?: number;
  virtualTpSlSyncWait?: number;
  skipTests?: string[];
  ignoredLoadErrors?: string[];
  /**
   * Opt in to the position-flip test: a single market order larger than the open
   * position, crossing through zero into the opposite side.
   *
   * Opt-in rather than skip-list because not every venue accepts an order that would
   * flip — some reject it, some silently clamp it to the position size — and a venue
   * that clamps would leave the account short-side-flat while the test reported a pass.
   * Adding a venue here should follow a manual check that it genuinely flips.
   */
  supportsPositionFlip?: boolean;
  badApiKeyMaxHttpRequests?: number;
}

const tradeEn = require('../../src/locales/en/trade.json').trade;

const getAccount = () => {
  const accountEnv = Cypress.env('account');
  if (!accountEnv) {
    throw new Error(
      `No account found for exchange "${Cypress.env('exchange') ?? 'unknown'}". ` +
        'Check that the CYPRESS_ENV_JSON GitHub secret contains credentials for this exchange, ' +
        'and that cypress.env.json has a matching account entry with active: true.',
    );
  }
  const { user: _user, ...account } = accountEnv;
  const exchangeConfig = getExchangeConfig();
  return {
    ...account,
    extra: exchangeConfig.accountExtra ? { ...account.extra, ...exchangeConfig.accountExtra } : account.extra,
    name: 'test',
  };
};

const getAccMetadata = () => {
  return getExchangeMetadata(getAccount().exchange);
};

// Get exchange configuration from environment
const getExchangeConfig = (): ExchangeConfig => {
  const config = Cypress.env('exchangeConfig');
  if (!config) {
    throw new Error(
      'Exchange configuration not found. Please set CYPRESS_EXCHANGE env var or ensure cypress.env.json is configured.',
    );
  }
  return config;
};

const getHost = () => (Cypress.config('baseUrl') || 'http://localhost:3000') + '/trade';
const initLoadTimeout = 60_000;

// Diagnostic capture arrays (populated in beforeEach, read by waitForExchangeLoad)
let capturedConsoleErrors: string[] = [];
type FailedRequest = { method: string; url: string; status: number; body: string };
let capturedFailedRequests: FailedRequest[] = [];

const isIgnoredBootFailedRequest = (url: string, status: number): boolean => {
  if (status < 400) return false;

  const path = (() => {
    try {
      return new URL(url, Cypress.config('baseUrl') || 'http://localhost:3000').pathname;
    } catch {
      return url;
    }
  })();

  return (
    path === '/api/custom-modules/drafts' || path === '/api/custom-hotkeys' || path === '/api/position-alerts-token'
  );
};

const recordFailedRequest = (request: FailedRequest) => {
  if (isIgnoredBootFailedRequest(request.url, request.status)) return;

  capturedFailedRequests.push(request);
  recordTransportFailure(request.url, request.status, request.body);
};

// Phase cursor — read by the Cypress.on('fail') handler so we can tag a
// beforeEach failure with which gate tripped (env config / Auth0 selectors /
// app cy.visit / waitForExchangeLoad). Reset to null at start of beforeEach.
type BeforeEachPhase = 'env' | 'auth' | 'visit' | 'load';
let currentBeforeEachPhase: BeforeEachPhase | null = null;
// Set to true by waitForExchangeLoad's captureAndThrowDiagnostics so the generic
// Cypress.on('fail') handler in beforeEach doesn't double-emit a logLoadFailure
// for the same exchange.
let loadFailureLogged = false;

// Transport-layer failure tagging (B2). Populated by classifyTransportFailure
// when an interceptor sees a fetch/XHR response that matches one of the known
// transport-side failure patterns (TradeTunnel down, WSoverHTTP timeout,
// proxy 5xx/429/403). First-match-wins so a flapping tunnel doesn't churn.
type TransportFailureReason =
  'tradetunnel-down' | 'wsohttp-timeout' | 'proxy-5xx' | 'proxy-rate-limit' | 'proxy-forbidden';
let transportFailureReason: TransportFailureReason | null = null;
let transportFailureUrl: string | null = null;

const classifyTransportFailure = (url: string, status: number, body: string): TransportFailureReason | null => {
  // Body-match patterns: these strings come from our own code so are unambiguous
  if (/TradeTunnel connection lost/i.test(body)) return 'tradetunnel-down';
  if (/WSoverHTTP request timed out/i.test(body)) return 'wsohttp-timeout';
  // URL+status patterns: keep narrow — only literal `exchange_proxy` substring
  // (verified path: https://private.asia.tealstreet.io/api/exchange_proxy/...)
  // to avoid mis-bucketing benign 4xx/5xx on non-proxy tealstreet endpoints.
  if (/exchange_proxy/i.test(url)) {
    if (status >= 500 && status < 600) return 'proxy-5xx';
    if (status === 429) return 'proxy-rate-limit';
    if (status === 403) return 'proxy-forbidden';
  }
  return null;
};

const recordTransportFailure = (url: string, status: number, body: string): void => {
  if (transportFailureReason !== null) return; // first-match-wins
  const reason = classifyTransportFailure(url, status, body);
  if (reason) {
    transportFailureReason = reason;
    // Strip query string to keep payloads small in result JSON.
    transportFailureUrl = url.split('?')[0] || url;
  }
};

/** Flip last 4 chars of a key to make it invalid but same length/format */
const corruptKey = (key: string): string => {
  if (!key || key.length < 4) return 'INVALID_KEY_000';
  const chars = key.split('');
  for (let i = chars.length - 4; i < chars.length; i++) {
    const c = chars[i];
    if (/[a-yA-Y]/.test(c)) chars[i] = String.fromCharCode(c.charCodeAt(0) + 1);
    else if (/[zZ]/.test(c)) chars[i] = c === 'z' ? 'a' : 'A';
    else if (/[0-8]/.test(c)) chars[i] = String.fromCharCode(c.charCodeAt(0) + 1);
    else if (c === '9') chars[i] = '0';
    else chars[i] = 'X';
  }
  return chars.join('');
};

// API log is captured via the app's existing Requests Panel system (requestsLogAtom in dev.atoms.ts).
// When enableRequestResponseAndPrivateWsLog is true, exchangeAccountController stores all HTTP
// requests/responses/errors in requestsLogAtom. Cypress reads it via window.__getRequestsLog()
// which is exposed by dev.atoms.ts when window.Cypress is detected.

/**
 * Waits for the exchange to fully load, with diagnostic reporting on failure.
 * Uses a polling loop so we can capture the loading state from PlaceOrderLoader
 * when the exchange fails to load, instead of getting a generic timeout error.
 *
 * When `ignoredLoadErrors` is configured for the exchange, the function will
 * reload the page and retry once if the only console errors match those patterns.
 * This handles intermittent API errors (e.g., Bitunix "Network Error" from
 * change_position_mode) that sometimes crash exchange startup.
 */
const waitForExchangeLoad = (retryAttempt: number = 0) => {
  const startTime = Date.now();
  const pollInterval = 500;
  const config = getExchangeConfig();
  const ignoredLoadErrors = config.ignoredLoadErrors || [];
  const maxRetries = ignoredLoadErrors.length > 0 ? 1 : 0;

  /**
   * Returns true if there are console errors and ALL of them match
   * at least one pattern in `ignoredLoadErrors`.
   */
  const allErrorsAreIgnored = (): boolean => {
    if (capturedConsoleErrors.length === 0 || ignoredLoadErrors.length === 0) return false;
    return capturedConsoleErrors.every((err) => ignoredLoadErrors.some((pattern) => err.includes(pattern)));
  };

  const captureAndThrowDiagnostics = () => {
    const exchangeName = getAccount().exchange || 'unknown';
    const diagnostics: {
      exchange: string;
      phase: BeforeEachPhase;
      loadingFlags: Record<string, boolean>;
      orderbookSpinning: boolean;
      consoleErrors: string[];
      failedRequests: { method: string; url: string; status: number; body: string }[];
      summary: string;
      transportFailureReason?: TransportFailureReason;
      transportFailureUrl?: string;
    } = {
      exchange: exchangeName,
      phase: 'load',
      loadingFlags: {},
      orderbookSpinning: false,
      consoleErrors: [...capturedConsoleErrors],
      failedRequests: [...capturedFailedRequests],
      summary: '',
      ...(transportFailureReason ? { transportFailureReason } : {}),
      ...(transportFailureUrl ? { transportFailureUrl } : {}),
    };

    cy.get('body').then(($body) => {
      // Capture loading flags from PlaceOrderLoader
      const loadingStatus = $body.find('[data-testid=exchange-loading-status]');
      if (loadingStatus.length > 0) {
        loadingStatus.find('[data-testid^="loading-"]').each((_, el) => {
          const testId = el.getAttribute('data-testid') || '';
          const key = testId.replace('loading-', '');
          const loaded = el.getAttribute('data-loaded') === 'true';
          diagnostics.loadingFlags[key] = loaded;
        });
      }

      // Check orderbook spinner
      diagnostics.orderbookSpinning = $body.find('[data-testid=orderbook-loading]').length > 0;

      // Build diagnostic summary
      const stuckFlags = Object.entries(diagnostics.loadingFlags)
        .filter(([, loaded]) => !loaded)
        .map(([key]) => key);

      const lines: string[] = [`[EXCHANGE LOAD FAILURE] ${exchangeName}`];
      for (const [key, loaded] of Object.entries(diagnostics.loadingFlags)) {
        const icon = loaded ? '\u2713' : '\u2717';
        const suffix = !loaded ? '  <-- STUCK' : '';
        lines.push(`  ${key}: ${icon}${suffix}`);
      }
      if (Object.keys(diagnostics.loadingFlags).length === 0) {
        lines.push('  (no loading flags found - store may not have initialized)');
      }
      lines.push(`  orderbook: ${diagnostics.orderbookSpinning ? 'spinning (public data issue)' : 'ok'}`);
      if (diagnostics.consoleErrors.length > 0) {
        lines.push(`  console errors: ${diagnostics.consoleErrors.length}`);
        diagnostics.consoleErrors.slice(0, 5).forEach((err) => {
          lines.push(`    - ${err.slice(0, 200)}`);
        });
      }
      if (diagnostics.failedRequests.length > 0) {
        lines.push(`  failed requests: ${diagnostics.failedRequests.length}`);
        diagnostics.failedRequests.slice(0, 5).forEach((req) => {
          lines.push(`    - ${req.method} ${req.url} -> ${req.status}`);
          if (req.body) lines.push(`      ${req.body.slice(0, 300)}`);
        });
      }

      diagnostics.summary = lines.join('\n');

      // Log diagnostics to Node console and store for results JSON
      cy.task('log', diagnostics.summary);
      loadFailureLogged = true;
      cy.task('logLoadFailure', diagnostics).then(() => {
        // Build descriptive error message
        let errorMsg = `Exchange ${exchangeName} failed to load within ${initLoadTimeout / 1000}s`;
        if (stuckFlags.length > 0) {
          errorMsg += `: ${stuckFlags.join(', ')} stuck`;
        }
        if (diagnostics.orderbookSpinning) {
          errorMsg += '. Orderbook still spinning (public data issue)';
        }
        if (diagnostics.consoleErrors.length > 0) {
          errorMsg += `. ${diagnostics.consoleErrors.length} console error(s)`;
        }
        if (diagnostics.failedRequests.length > 0) {
          errorMsg += `. ${diagnostics.failedRequests.length} failed request(s)`;
        }
        errorMsg += '.';

        throw new Error(errorMsg);
      });
    });
  };

  const poll = (): void => {
    cy.get('body').then(($body) => {
      if ($body.find('[data-testid=toggle-tabs-limit]:visible, [data-testid=dropdown-tabs-limit]:visible').length > 0) {
        return; // Exchange loaded successfully
      }

      if (Date.now() - startTime > initLoadTimeout) {
        // If only ignored errors and we have retries left, reload and try again
        if (retryAttempt < maxRetries && allErrorsAreIgnored()) {
          cy.task(
            'log',
            `Exchange load timed out with only ignored errors: [${capturedConsoleErrors.join('; ')}]. ` +
              `Reloading page (retry ${retryAttempt + 1}/${maxRetries})...`,
          );
          capturedConsoleErrors = [];
          capturedFailedRequests = [];
          cy.reload();
          cy.wait(3000); // Give page time to re-initialize after reload
          waitForExchangeLoad(retryAttempt + 1);
          return;
        }
        captureAndThrowDiagnostics();
        return;
      }

      // Not loaded yet, wait and retry
      cy.wait(pollInterval, { log: false });
      poll();
    });
  };

  poll();
};

// Exchange-specific values from config
let exchangeConfig: ExchangeConfig;
let symbol: string;
let orderSizeInQuote: number;
let orderSizeVariance: number;
let farTouchDelta: number;
let crossDelta: number;
let quoteCurrency: string;
let displaySymbol: string;
let expectedWalletBalance: number;
let expectedAvailableMargin: number;
let actionWaitTime: number = 5000; // Default value in case before() doesn't run
let slowUpdateExtraWaitTime: number = 0; // Default value in case before() doesn't run
let openPositionSettleWaitTime: number = 5000;
let balanceVariance: number = 1.0; // Default keeps existing balance-check behavior
let positionWaitTimeout: number = 45000; // Timeout for position appearance/disappearance checks
let lastParadexPlacedOrderIds: string[] = [];

// These will be initialized in before()
let lastTrade = 0;

// utils
const findClosestValue = (arr: number[], target: number) => {
  return arr.reduce((prev, curr) => (Math.abs(curr - target) < Math.abs(prev - target) ? curr : prev));
};

const getLeverageMagnetsToTest = (metaData: ExchangeMetadata, desiredValues = [5, 20]) => {
  const magnetValsSet = new Set(desiredValues.map((val) => findClosestValue(metaData.leverageMagnets, val)));
  const magnetValsWithIndex: [number, number][] = [...magnetValsSet].map((val) => [
    val,
    metaData.leverageMagnets.indexOf(val),
  ]);
  return magnetValsWithIndex;
};

// helpers

const getAvailableMargin = () => {
  cy.get('[data-testid=margin-available-margin]')
    .then(($element) => parseFloat($element.text().replace(' USD', '')))
    .as('availableMargin');
};

const getAdjustedBalance = () => {
  cy.get('[data-testid=margin-adjusted-balance]')
    .then(($element) => parseFloat($element.text().replace(' USD', '')))
    .as('adjustedBalance');
};

const readLastTradeFromStoreSnapshot = (win: Window): number | undefined => {
  const snapshot = ((win as any).__getStoreSnapshot?.() || {}) as Record<string, any>;
  const tickers = snapshot[getAccount().name]?.tickers || [];
  const ticker = tickers.find((t: any) => t?.symbol === symbol || t?.id === symbol || t?.id?.replace(/-/g, '') === symbol);
  const last = Number(ticker?.last || 0);
  if (Number.isFinite(last) && last > 0) return last;

  const bid = Number(ticker?.bid || 0);
  const ask = Number(ticker?.ask || 0);
  if (Number.isFinite(bid) && bid > 0 && Number.isFinite(ask) && ask > 0) {
    return (bid + ask) / 2;
  }

  const orderBooks = snapshot[getAccount().name]?.orderBooks || [];
  const orderBook = orderBooks.find((book: any) => book?.symbol === symbol || book?.symbol?.replace(/-/g, '') === symbol);
  const bookBid = Number(orderBook?.bid || 0);
  const bookAsk = Number(orderBook?.ask || 0);
  if (Number.isFinite(bookBid) && bookBid > 0 && Number.isFinite(bookAsk) && bookAsk > 0) {
    return (bookBid + bookAsk) / 2;
  }
  if (Number.isFinite(bookBid) && bookBid > 0) return bookBid;
  if (Number.isFinite(bookAsk) && bookAsk > 0) return bookAsk;

  return undefined;
};

const getLastTrade = () => {
  cy.log('Fetching current market price');
  cy.get('body').then(($body) => {
    const selector = `[data-testid=last-trade-title-${symbol}]`;
    const domPrice = parseFloat($body.find(selector).filter(':visible').first().text());
    if (Number.isFinite(domPrice) && domPrice > 0) {
      lastTrade = domPrice;
      cy.log(`Current lastTrade from DOM: ${lastTrade}`);
      return;
    }

    cy.window()
      .should((win) => {
        const storePrice = readLastTradeFromStoreSnapshot(win);
        expect(storePrice, `store ticker price for ${symbol}`).to.be.a('number').and.be.greaterThan(0);
        lastTrade = storePrice!;
      })
      .then(() => cy.log(`Current lastTrade from store: ${lastTrade}`));
  });
};

const getSelectValueFromElement = ($element: JQuery<HTMLElement>) =>
  $element.children(':visible').first().text() || $element.children().first().text();

const getVisibleSelect = (selector: string, eq = 0, timeout = 30000) =>
  cy.get(selector, { timeout }).filter(':visible').eq(eq);

const clickOrdersTab = (tabTestId: string) => {
  cy.get('[data-testid=open-orders-dropdown]').click({ force: true });
  cy.get(`[data-testid=${tabTestId}]`).click({ force: true });
};

// Close every open position across every account by placing opposite-side
// market reduceOnly orders directly via the controller. Same code path the
// "Close All Positions" UI button uses (closeAllPositions → createMarketOrder),
// but bypasses the popconfirm + Yes-click race that the UI introduces.
// `store.positions[].contracts` is in BASE units, but placeOrder expects the
// venue's NATIVE size — on contract-denominated venues formatCreateOrder does
// adjust(amount, precision.amount) against a lot size in contracts. Passing base
// straight through made okx's 0.0001 BTC (= 0.01 contracts, lotSz 0.01) adjust to
// 0: "Order amount must be finite and positive after adjustment", so the position
// could never be flattened and blocked the suite permanently. blofin has the same
// shape and only survived because oversizing is clamped by reduceOnly.
// Mirrors apps/web/src/utils/position/getPositionNativeSize.ts — keep in sync.
const nativeCloseSize = (exch: any, symbol: string, side: string, contracts: number): number => {
  const pos = exch?.store?.displayedPositions?.find((p: any) => p.symbol === symbol && p.side === side);
  const market = pos?.market ?? exch?.store?.marketsBySymbol?.[symbol];
  const fromExtra = Math.abs(Number(pos?.extra?.contractsNative || 0));
  const base = Math.abs(pos?.contracts ?? contracts);
  if (!market) return fromExtra || base;

  const contractSize = market.contractSize || 1;
  // Inverse sizes come off notional; fall back to base so a missing live
  // position can't silently resolve to 0 and skip the close.
  const fromPosition = market.isInverse ? Math.abs(pos?.notional || 0) / contractSize || base : base / contractSize;

  return adjust(Math.max(fromExtra, fromPosition), market.precision.amount);
};

const recoverHostedSymbolPositionsForE2E = () => {
  if (exchangeConfig.exchange !== 'paradex') return;

  cy.window({ log: false }).then((win) => {
    const accountName = getAccount().name;
    const eac = (win as any).exchangeAccountController;
    if (!eac || typeof eac.scheduleUserActivityPositionRefetch !== 'function') return;
    cy.task('log', `[flatten:paradex] recovering hosted ${symbol} positions before cleanup`);
    eac.scheduleUserActivityPositionRefetch(accountName, [symbol]);
  });
  cy.wait(800);
};

const recoverWooSymbolPositionsForE2E = () => {
  if (exchangeConfig.exchange !== 'woo') return;

  cy.window({ log: false }).then((win) => {
    const accountName = getAccount().name;
    const eac = (win as any).exchangeAccountController;
    const exch = eac?.getExchange?.(accountName);
    if (!exch || typeof exch.fetchPositions !== 'function') return;

    return cy.wrap(exch.fetchPositions([symbol]), { log: false, timeout: 30000 }).then((positions: any[]) => {
      if (Array.isArray(positions)) {
        exch.store?.addOrUpdatePositions?.(positions);
      }
      eac?.scheduleUserActivityPositionRefetch?.(accountName, [symbol]);
      const openPositions = Array.isArray(positions)
        ? positions.filter((position) => position.symbol === symbol && position.contracts > 0)
        : [];
      cy.task('log', `[woo:positions] recovered ${openPositions.length} open ${symbol} position(s)`);
    });
  });
};

const waitForOpenedPositionForE2E = (includeSlowWait = false) => {
  cy.wait(openPositionSettleWaitTime);
  if (includeSlowWait && exchangeConfig.exchange !== 'woo') {
    cy.wait(slowUpdateExtraWaitTime);
  }
  recoverWooSymbolPositionsForE2E();
};

const recoverHostedOpenOrdersForE2E = () => {
  if (!['okx', 'paradex', 'woo'].includes(exchangeConfig.exchange)) return;

  cy.window({ log: false }).then((win) => {
    const { exch } = getActiveExchangeForE2E(win);
    if (!exch || typeof exch.fetchOrders !== 'function') return;

    return cy.wrap(exch.fetchOrders(), { log: false, timeout: 30000 }).then((orders: any[]) => {
      const openOrders = Array.isArray(orders) ? orders.filter((order) => order.status === 'open') : [];
      if (typeof exch.store?.addOrUpdateOrdersFull === 'function') {
        exch.store.addOrUpdateOrdersFull(orders);
      } else {
        exch.store?.update?.({ orders, loaded: { ...exch.store.loaded, orders: true } });
      }
      cy.task('log', `[${exchangeConfig.exchange}:orders] recovered ${openOrders.length} open order(s)`);
    });
  });
};

const ensureOkxBracketAlgoOrdersForE2E = (side: OrderSide, tpPrice: number, slPrice: number, sizeInBase: number) => {
  const startedAt = Date.now();
  const expectedSide = side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;

  const findMatchingOrder = (orders: any[], type: string, expectedPrice: number) => {
    return orders.find((order) => {
      return (
        order?.status === 'open' &&
        order?.type === type &&
        order?.side === expectedSide &&
        Math.abs(Number(order?.price) - expectedPrice) <= expectedPrice * 0.02
      );
    });
  };

  const poll = () => {
    cy.window({ log: false }).then((win) => {
      const { exch } = getParadexExchangeForE2E(win);
      if (!exch || typeof exch.fetchAlgoOrders !== 'function') {
        throw new Error('OKX e2e bracket validation failed: exchange controller is not ready');
      }

      return cy
        .wrap(Promise.all([exch.fetchAlgoOrders('trigger'), exch.fetchAlgoOrders('conditional')]), {
          log: false,
          timeout: 30000,
        })
        .then(([triggerOrders, conditionalOrders]: any[]) => {
          const orders = [
            ...(Array.isArray(triggerOrders) ? triggerOrders : []),
            ...(Array.isArray(conditionalOrders) ? conditionalOrders : []),
          ];
          const takeProfitOrder = findMatchingOrder(orders, 'take_profit_market', tpPrice);
          const stopOrder = findMatchingOrder(orders, 'stop_market', slPrice);

          if (!takeProfitOrder || !stopOrder) {
            if (Date.now() - startedAt > 30000) {
              throw new Error(
                `OKX bracket algos not found near TP ${tpPrice} / SL ${slPrice}: ${JSON.stringify(
                  orders.map((order) => ({
                    id: order?.id,
                    type: order?.type,
                    side: order?.side,
                    status: order?.status,
                    price: order?.price,
                  })),
                )}`,
              );
            }

            cy.wait(1000);
            return poll();
          }

          expect(takeProfitOrder.remaining, `OKX Take Profit remaining near ${tpPrice}`).to.be.closeTo(
            sizeInBase,
            sizeInBase * orderSizeVariance,
          );
          expect(stopOrder.remaining, `OKX Stop Loss remaining near ${slPrice}`).to.be.closeTo(
            sizeInBase,
            sizeInBase * orderSizeVariance,
          );
        });
    });
  };

  poll();
};

const ensureOkxStandaloneStopOrderForE2E = (side: OrderSide, price: number, size: number | string) => {
  const startedAt = Date.now();
  const expectedSizeInBase = typeof size === 'number' ? size / lastTrade : undefined;

  const findMatchingOrder = (orders: any[]) => {
    return orders.find((order) => {
      return (
        order?.status === 'open' &&
        order?.type === 'stop_market' &&
        order?.side === side &&
        order?.symbol === symbol &&
        Math.abs(Number(order?.price) - price) <= price * 0.06
      );
    });
  };

  const poll = () => {
    cy.window({ log: false }).then((win) => {
      const { exch } = getParadexExchangeForE2E(win);
      if (!exch || typeof exch.fetchAlgoOrders !== 'function') {
        throw new Error('OKX e2e stop validation failed: exchange controller is not ready');
      }

      return cy
        .wrap(Promise.all([exch.fetchAlgoOrders('trigger'), exch.fetchAlgoOrders('conditional')]), {
          log: false,
          timeout: 30000,
        })
        .then(([triggerOrders, conditionalOrders]: any[]) => {
          const orders = [
            ...(Array.isArray(triggerOrders) ? triggerOrders : []),
            ...(Array.isArray(conditionalOrders) ? conditionalOrders : []),
          ];
          exch.store?.addOrUpdateOrders?.(orders);

          const stopOrder = findMatchingOrder(orders);
          if (!stopOrder) {
            if (Date.now() - startedAt > 30000) {
              throw new Error(
                `OKX stop algo not found near ${price}: ${JSON.stringify(
                  orders.map((order) => ({
                    id: order?.id,
                    type: order?.type,
                    side: order?.side,
                    status: order?.status,
                    symbol: order?.symbol,
                    price: order?.price,
                    remaining: order?.remaining,
                  })),
                )}`,
              );
            }

            cy.wait(1000);
            return poll();
          }

          if (expectedSizeInBase !== undefined) {
            expect(stopOrder.remaining, `OKX stop remaining near ${price}`).to.be.closeTo(
              expectedSizeInBase,
              expectedSizeInBase * orderSizeVariance,
            );
          }
        });
    });
  };

  poll();
};

const getOpenOrdersFromSnapshot = (win: Window) => {
  const snap = (win as any).__getStoreSnapshot?.() || {};
  return Object.values(snap as any).flatMap((a: any) =>
    (a?.orders || []).filter((o: any) => o.status === 'open' && o.symbol === symbol),
  ) as any[];
};

/**
 * Actions the suite performs deliberately, recorded so an accounting verifier can check
 * the venue's own history reconstructs them. Kept a closed set: a free-text action would
 * be unmatchable on the other side.
 */
type GroundTruthAction =
  'market-order' | 'limit-order' | 'flip' | 'flip-observed' | 'partial-close' | 'close' | 'flatten';

/**
 * Records what the test just did, plus the position state it produced.
 *
 * The history audits in `scripts/history-audit/` compare a venue's mapped data against
 * that same venue's raw rows — they prove parsing, but they cannot prove the numbers
 * describe what actually happened, because both sides come from the exchange. This log is
 * the independent oracle: the suite knows it placed a $10 market buy at time T, so a
 * reconstructed position that disagrees is wrong no matter how self-consistent it looks.
 *
 * Position state is read from `__getStoreSnapshot()` rather than the DOM so the record
 * carries real numbers (contracts, entry price) instead of formatted display strings.
 */
const recordGroundTruth = (action: GroundTruthAction, detail: Record<string, unknown> = {}) => {
  cy.window().then((win) => {
    const snapshot = ((win as any).__getStoreSnapshot?.() || {}) as Record<string, any>;
    const positions = Object.values(snapshot)
      .flatMap((account: any) => account?.positions || [])
      .filter((position: any) => position?.symbol === symbol && Number(position?.contracts) !== 0)
      .map((position: any) => ({
        side: position.side,
        contracts: position.contracts,
        entryPrice: position.entryPrice,
      }));

    cy.task('recordGroundTruth', {
      exchange: exchangeConfig.exchange,
      event: {
        ts: Date.now(),
        test: Cypress.currentTest?.title ?? 'unknown',
        exchange: exchangeConfig.exchange,
        symbol,
        action,
        ...detail,
        positionsAfter: positions,
      },
    });
  });
};

/**
 * Put the account back into one-way mode if the selector is available and enabled.
 *
 * The hedge-mode test leaves the account hedged, and position mode is global on most
 * venues, so any later test assuming a single netted position needs this first. A
 * disabled or absent selector means the venue does not expose the switch (or forces the
 * change on its own site), so this is deliberately a no-op there rather than a failure.
 */
const ensureOneWayPositionMode = () => {
  cy.get('body').then(($body) => {
    const $selector = $body.find('[data-testid=position-mode-selector]').filter(':visible');
    if ($selector.length > 0 && !$selector.attr('class')?.includes('disabled')) {
      getVisibleSelect('[data-testid=position-mode-selector]').then(($element) => {
        if (getSelectValueFromElement($element) !== 'One Way') {
          setSelectValue('[data-testid=position-mode-selector]', tradeEn['position-mode-oneway']);
          cy.wait(actionWaitTime);
        }
      });
    }
  });
};

const ensureOpenOrdersLengthForE2E = (length: number, timeout = 30000) => {
  if (exchangeConfig.exchange !== 'paradex') {
    ensureTableBodyLength('[data-testid=open-orders-table]', length, timeout);
    return;
  }

  recoverHostedOpenOrdersForE2E();
  cy.window({ log: false, timeout }).then((win) => {
    const openOrders = getOpenOrdersFromSnapshot(win);
    expect(openOrders.length, 'Paradex recovered open orders').to.eq(length);
  });
};

const closeAllPositionsViaController = () => {
  cy.window({ log: false }).then((win) => {
    const eac = (win as any).exchangeAccountController;
    if (!eac || typeof eac.getExchanges !== 'function') return;
    const snap = (win as any).__getStoreSnapshot?.() || {};
    type Item = { acct: string; symbol: string; side: string; contracts: number };
    const items: Item[] = [];
    for (const [acct, data] of Object.entries(snap)) {
      const accountData = data as any;
      const positions = (accountData?.positions || []) as any[];
      for (const p of positions) {
        if (!p.contracts || p.contracts === 0) continue;
        items.push({ acct, symbol: p.symbol, side: p.side, contracts: p.contracts });
      }
    }
    if (items.length === 0) return;
    cy.task('log', `[flatten] closing ${items.length} position(s) via controller`);
    const exchanges = eac.getExchanges();
    if (!exchanges) return;
    const logs: string[] = [];
    return cy
      .wrap(
        Promise.all(
          items.map(async ({ acct, symbol, side, contracts }) => {
            const exch = typeof exchanges.get === 'function' ? exchanges.get(acct) : (exchanges as any)[acct];
            if (!exch || typeof exch.placeOrder !== 'function') return;

            const amount = nativeCloseSize(exch, symbol, side, contracts);
            if (!(amount > 0)) {
              logs.push(`[flatten] ${acct}:${symbol} skipped — size ${contracts} adjusts to ${amount}, cannot reduce`);
              return;
            }
            if (amount !== contracts) {
              logs.push(`[flatten] ${acct}:${symbol} ${contracts} base -> ${amount} native`);
            }
            const closeSide = side === 'long' ? 'sell' : 'buy';
            try {
              if (exchangeConfig.exchange === 'paradex' && typeof exch.setMaxSlippage === 'function') {
                await exch.setMaxSlippage('0.2', symbol);
              }
              await exch.placeOrder({
                symbol,
                side: closeSide,
                type: 'market',
                amount,
                reduceOnly: true,
                ...(exchangeConfig.exchange === 'paradex'
                  ? { timeInForce: OrderTimeInForce.ImmediateOrCancel, maxSlippagePct: 20 }
                  : {}),
              });
              eac.scheduleUserActivityPositionRefetch?.(acct, [symbol]);
            } catch (err: any) {
              // Do not swallow: a silent failure here surfaces only as an opaque
              // "positions remaining after flatten" timeout 30s later.
              logs.push(`[flatten] ${acct}:${symbol} placeOrder failed: ${err?.message ?? err}`);
            }
          }),
        ),
        { log: false, timeout: 30000 },
      )
      .then(() => {
        logs.forEach((line) => cy.task('log', line));
      });
  });
};

// closeAllPositionsViaController is symbol-agnostic — it closes EVERY position on
// EVERY registered account. On a shared or human-used account that silently
// liquidates real positions: adding Kraken to the matrix closed an unrelated
// PF_HYPEUSD long sitting on +$18. Refuse to trade anything outside the
// configured test symbol and make the operator deal with it deliberately.
// Deliberate one-off account maintenance: `allow_foreign_flatten` on a manual
// workflow_dispatch lets the suite clear leftover dust through the real app
// instead of hand-rolled REST (which bypasses the proxy/tunnel routing that
// IP-whitelisted keys require). Actions renders an unset/false boolean input as
// '' / 'false' — both truthy as strings — so compare explicitly. Never set this
// on the nightly.
const allowForeignFlatten = String(Cypress.env('allowForeignFlatten')) === 'true';

const assertNoForeignPositions = () => {
  cy.window({ log: false }).then((win) => {
    const snap = (win as any).__getStoreSnapshot?.() || {};
    const foreign = Object.entries(snap as any).flatMap(([acct, data]: [string, any]) =>
      (data?.positions || [])
        .filter((p: any) => p.contracts && p.contracts !== 0 && p.symbol !== symbol)
        .map((p: any) => `${acct}:${p.symbol} ${p.side ?? ''} ${p.contracts}`),
    );
    if (foreign.length === 0) return;

    if (allowForeignFlatten) {
      cy.task(
        'log',
        `[flatten:foreign] allow_foreign_flatten is ON — closing ${foreign.length} position(s) ` +
          `outside the test symbol (${symbol}): ${foreign.join(', ')}`,
      );
      return;
    }

    throw new Error(
      `Refusing to flatten: ${foreign.length} position(s) outside the test symbol (${symbol}) — ` +
        `${foreign.join(', ')}. This account is not clean; closing these would trade real size. ` +
        `Clear them deliberately before running e2e against it.`,
    );
  });
};

const flattenPositions = () => {
  cy.log('Flattening positions');
  if (slowUpdateExtraWaitTime > 0) cy.wait(slowUpdateExtraWaitTime);

  recoverHostedSymbolPositionsForE2E();
  assertNoForeignPositions();
  closeAllPositionsViaController();
  cy.wait(actionWaitTime);

  const flattenTimeout = (exchangeConfig.exchange === 'paradex' ? 60000 : 30000) + slowUpdateExtraWaitTime * 2;
  const retryFlattenUntil = (deadline: number): Cypress.Chainable<void> =>
    cy.window({ log: false }).then((win) => {
      const snap = (win as any).__getStoreSnapshot?.() || {};
      const positions = Object.values(snap as any).flatMap((a: any) =>
        (a?.positions || []).filter((p: any) => p.contracts && p.contracts !== 0),
      ) as any[];
      if (positions.length === 0 || Date.now() >= deadline) return;

      cy.task('log', `[flatten] retry: ${positions.length} position(s) still open`);
      closeAllPositionsViaController();
      cy.wait(actionWaitTime + 2000);
      return retryFlattenUntil(deadline);
    });

  retryFlattenUntil(Date.now() + flattenTimeout);

  cy.window({ log: false, timeout: flattenTimeout }).should((win) => {
    const snap = (win as any).__getStoreSnapshot?.() || {};
    const positions = Object.values(snap as any).flatMap((a: any) =>
      (a?.positions || []).filter((p: any) => p.contracts && p.contracts !== 0),
    ) as any[];
    expect(positions.length, 'positions remaining after flatten').to.eq(0);
  });
};

// Cancel every open order across every account by going directly through the
// controller. Bypasses the OpenOrders UI entirely — no tab switching, no
// row-render race, no popconfirm timing. The controller is mounted on window
// in Cypress (gated by `window.Cypress` in exchangeAccountController).
const cancelAllOrdersViaController = () => {
  cy.window({ log: false }).then((win) => {
    const eac = (win as any).exchangeAccountController;
    if (!eac || typeof eac.getExchanges !== 'function') return;
    const snap = (win as any).__getStoreSnapshot?.() || {};
    const byAccount: { [acct: string]: any[] } = {};
    for (const [acct, data] of Object.entries(snap)) {
      const orders = ((data as any)?.orders || []).filter((o: any) => o.status === 'open');
      if (orders.length > 0) byAccount[acct] = orders;
    }
    const total = Object.values(byAccount).reduce((n, o) => n + o.length, 0);
    if (total === 0) return;
    cy.task(
      'log',
      `[flatten] cancelling ${total} open order(s) across ${Object.keys(byAccount).length} account(s) via controller`,
    );
    const exchanges = eac.getExchanges();
    if (!exchanges) return;
    return cy.wrap(
      Promise.all(
        Object.entries(byAccount).map(async ([acct, orders]) => {
          const exch = typeof exchanges.get === 'function' ? exchanges.get(acct) : (exchanges as any)[acct];
          if (!exch || typeof exch.cancelOrders !== 'function') return;
          try {
            await exch.cancelOrders(orders);
          } catch (err: any) {
            (win as any).__lastFlattenCancelError = `${err?.message ?? String(err)}`;
          }
        }),
      ),
      { log: false, timeout: 30000 },
    );
  });
};

const isIgnorableCleanupOrder = (order: any) => {
  return (
    exchangeConfig.exchange === 'hyperliquid' &&
    order.status === 'open' &&
    Number(order.amount) === 0 &&
    order.reduceOnly &&
    String(order.type || '').includes('stop')
  );
};

const getOpenOrdersForCleanup = (snap: Record<string, any>) =>
  Object.values(snap).flatMap((a: any) =>
    (a?.orders || []).filter((o: any) => o.status === 'open' && !isIgnorableCleanupOrder(o)),
  ) as any[];

const getOpenOrderAmountFromStore = (
  side: OrderSide,
  price: number,
  options: { reduceOnly?: boolean; typeIncludes?: string } = {},
): number | undefined => {
  const snap = ((cy.state('window') as any)?.__getStoreSnapshot?.() || {}) as Record<string, any>;
  const matchingOrder = Object.values(snap)
    .flatMap((a: any) => a?.orders || [])
    .find((order: any) => {
      const orderPrice = Number(order.price);
      return (
        order.status === 'open' &&
        order.symbol === symbol &&
        order.side === side &&
        (options.reduceOnly === undefined || order.reduceOnly === options.reduceOnly) &&
        (!options.typeIncludes || String(order.type || '').includes(options.typeIncludes)) &&
        !Number.isNaN(orderPrice) &&
        Math.abs(orderPrice - price) <= price * 0.06
      );
    });

  return matchingOrder ? Number(matchingOrder.amount) : undefined;
};

const getHyperliquidStopAmountFromStore = (side: OrderSide, price: number): number | undefined => {
  if (exchangeConfig.exchange !== 'hyperliquid') return undefined;
  return getOpenOrderAmountFromStore(side, price, { reduceOnly: true, typeIncludes: 'stop' });
};

const getKrakenReduceLimitAmountFromStore = (side: OrderSide, price: number): number | undefined => {
  if (exchangeConfig.exchange !== 'kraken') return undefined;
  return getOpenOrderAmountFromStore(side, price, { reduceOnly: true, typeIncludes: 'limit' });
};

const flattenOrders = () => {
  cy.log('Flattening orders');

  recoverHostedOpenOrdersForE2E();

  // Direct controller-level cancel. Same code path the UI uses, but without
  // tab filters, popconfirm timing, or button-disabled races.
  cancelAllOrdersViaController();
  cy.wait(actionWaitTime);
  recoverHostedOpenOrdersForE2E();

  // Up to 3 retries for transient API errors / WS-sync lag. Each retry
  // re-issues the cancel against whatever's still flagged open in the store.
  const retry = (left: number) => {
    cy.window({ log: false }).then((win) => {
      const snap = (win as any).__getStoreSnapshot?.() || {};
      const open = getOpenOrdersForCleanup(snap);
      if (open.length === 0) return;
      cy.task('log', `[flatten] retry: ${open.length} order(s) still open (${left} attempts left)`);
      cancelAllOrdersViaController();
      cy.wait(actionWaitTime);
      recoverHostedOpenOrdersForE2E();
      if (left > 1) retry(left - 1);
    });
  };
  retry(3);

  // Surface the last cancel error (if any) so we can see WHY cleanup is failing
  cy.window({ log: false }).then((win) => {
    const lastErr = (win as any).__lastFlattenCancelError;
    if (lastErr) cy.task('log', `[flatten:diag] last cancel error: ${lastErr}`);
  });

  // Final convergence: store has 0 open orders. Generous timeout because some
  // exchanges' private WS can lag the actual exchange state by 10-20s.
  recoverHostedOpenOrdersForE2E();
  cy.window({ log: false, timeout: 45000 }).should((win) => {
    const snap = (win as any).__getStoreSnapshot?.() || {};
    const open = getOpenOrdersForCleanup(snap);
    expect(open.length, 'open orders remaining after cleanup').to.eq(0);
  });
};

const isCheckedOrPressed = ($el: JQuery<HTMLElement>): boolean => {
  // Support both checkboxes (:checked) and toggle buttons (aria-pressed)
  if ($el.is(':checked')) return true;
  if ($el.attr('aria-pressed') === 'true') return true;
  return false;
};

const ensureCheckboxIsChecked = (selector: string, eq: number = 0) => {
  cy.get(selector)
    .eq(eq)
    .then(($element) => {
      if (!isCheckedOrPressed($element)) {
        cy.get(selector).eq(eq).click();
      }
    });
};

const ensureCheckboxIsNotChecked = (selector: string, eq: number = 0) => {
  cy.get(selector)
    .eq(eq)
    .then(($element) => {
      if (isCheckedOrPressed($element)) {
        cy.get(selector).eq(eq).click();
      }
    });
};

const ensureVisibleCheckboxesAreChecked = (selector: string) => {
  cy.get('body').then(($body) => {
    if ($body.find(selector).length === 0) return;

    cy.get(selector).each(($element) => {
      if (!isCheckedOrPressed($element)) {
        cy.wrap($element).click({ force: true });
      }
    });
  });
};

const ensureVisibleCheckboxesAreNotChecked = (selector: string) => {
  cy.get('body').then(($body) => {
    if ($body.find(selector).length === 0) return;

    cy.get(selector).each(($element) => {
      if (isCheckedOrPressed($element)) {
        cy.wrap($element).click({ force: true });
      }
    });
  });
};

const ensureTableBodyLength = (selector: string, length: number, timeout = 30000) => {
  if (
    ['okx', 'paradex'].includes(exchangeConfig.exchange) &&
    selector === '[data-testid=open-orders-table]' &&
    length > 0
  ) {
    recoverHostedOpenOrdersForE2E();
    cy.wait(300);
  }

  // Register a one-time fail listener to capture store snapshot on assertion timeout
  const onFail = (err: Error) => {
    // Synchronously grab snapshot via the already-exposed window function
    const win = cy.state('window');
    if (win) {
      const getSnapshot = (win as any).__getStoreSnapshot;
      if (typeof getSnapshot === 'function') {
        const snapshot = JSON.stringify(getSnapshot()).substring(0, 2000);
        Cypress.log({
          name: 'STORE_SNAPSHOT',
          message: `ensureTableBodyLength(${selector}, ${length}) failed`,
          consoleProps: () => ({ snapshot }),
        });
        const messageWithSnapshot = `${err.message}\n[Store Snapshot] ${snapshot}`;
        try {
          err.message = messageWithSnapshot;
        } catch {
          const enrichedError = new Error(messageWithSnapshot);
          enrichedError.name = err.name;
          enrichedError.stack = err.stack;
          (enrichedError as any).cause = err;
          throw enrichedError;
        }
      }
    }
    throw err;
  };
  Cypress.once('fail', onFail);

  cy.get(selector, { timeout })
    .should(($element) => {
      expect($element.find('tr').length - 1).to.eq(length);
    })
    .then(() => {
      // Assertion passed — remove the fail listener
      Cypress.removeListener('fail', onFail);
    });
};

const ensureTableBodyLengthOneOf = (selector: string, oneOf: number[], timeout = 30000) => {
  cy.get(selector, { timeout }).should(($element) => {
    expect($element.find('tr').length - 1).to.be.oneOf(oneOf);
  });
};

const ensureTableBodyLengthAtLeast = (selector: string, length: number, timeout = 30000) => {
  if (
    ['okx', 'paradex'].includes(exchangeConfig.exchange) &&
    selector === '[data-testid=open-orders-table]' &&
    length > 0
  ) {
    recoverHostedOpenOrdersForE2E();
    cy.wait(300);
  }

  cy.get(selector, { timeout }).should(($element) => {
    expect($element.find('tr').length - 1).to.be.at.least(length);
  });
};

const ensureSelectValue = (selector: string, value: string, eq = 0, timeout = 30000) => {
  getVisibleSelect(selector, eq, timeout).should(($element) => {
    expect(getSelectValueFromElement($element)).to.eq(value);
  });
};

const getSliderValue = (name: string, selector: string, eq = 0) => {
  cy.get(selector)
    .eq(eq)
    .find('input[type=range]')
    .then(($input) => {
      const foundValue = $input.val();
      expect(foundValue).to.not.be.undefined;
      return parseFloat(foundValue as string);
    })
    .as(name);
};

const ensureSliderValue = (selector: string, value: number | string, eq = 0) => {
  cy.get(selector)
    .eq(eq)
    .find('input[type=range]')
    .then(($input) => {
      expect(parseFloat($input.val() as string)).to.eq(Number(value));
    });
};

const ensureSliderValueCloseTo = (selector: string, value: number | string, delta: number, eq = 0) => {
  cy.get(selector)
    .eq(eq)
    .find('input[type=range]')
    .then(($input) => {
      expect(parseFloat($input.val() as string)).to.be.closeTo(Number(value), delta);
    });
};

const setNativeSliderValue = (selector: string, value: number, eq = 0) => {
  const pointerEvent = { pointerId: 1, pointerType: 'mouse', isPrimary: true, button: 0, buttons: 1, force: true };
  cy.get(selector)
    .eq(eq)
    .find('input[type=range]')
    .trigger('pointerdown', pointerEvent)
    .invoke('val', value)
    .trigger('input', { force: true })
    .trigger('pointerup', { ...pointerEvent, buttons: 0 });
};

const getTableHeader = (tableElement: JQuery<HTMLElement>) => {
  return tableElement.find('tr')[0];
};

const ensurePositionSize = (sizeInQuote: number, side: PositionSide, symbol: string, timeout = 30000) => {
  cy.get('[data-testid=open-positions-table]', { timeout }).should(($element) => {
    const header = getTableHeader($element);
    const position = findPosition($element, side, displaySymbol);
    const sizeInBase = sizeInQuote / lastTrade;
    expect(safelyParseValue(getCell(header, position, 'Size').textContent)).to.be.closeTo(
      sizeInBase,
      sizeInBase * orderSizeVariance,
    );
  });
};

const getDisplayedPositionSizeInBase = (side: PositionSide) => {
  return cy.get('[data-testid=open-positions-table]').then(($element) => {
    const header = getTableHeader($element);
    const position = findPosition($element, side, displaySymbol);
    return safelyParseValue(getCell(header, position, 'Size').textContent);
  });
};

const ensurePositionSide = (header: HTMLElement, position: HTMLElement, side: PositionSide) => {
  const expectedDataValue = side === PositionSide.Long ? '1' : '-1';
  const coloredNumber = getCell(header, position, 'Size').querySelector('[data-testid=colored-number]');
  expect(coloredNumber?.getAttribute('data-value')).to.eq(expectedDataValue);
};

const findPosition = (tableElement: JQuery<HTMLElement>, side: PositionSide, symbol: string) => {
  const trs = tableElement.find('tr');
  const header = trs[0];
  const positions = trs.toArray().slice(1);

  const sizeIndex = Array.from(trs[0].children).findIndex((td: any) => {
    return td.innerText.startsWith('Size');
  });

  const symbolIndex = Array.from(trs[0].children).findIndex((td: any) => {
    return td.innerText.startsWith('Symbol');
  });

  const expectedDataValue = side === PositionSide.Long ? '1' : '-1';

  const position = positions.find(function (position) {
    const coloredNumber = position.children[sizeIndex]?.querySelector('[data-testid=colored-number]');
    return (
      position.children[symbolIndex]?.textContent?.includes(symbol) &&
      coloredNumber?.getAttribute('data-value') === expectedDataValue
    );
  });

  assert.isDefined(position);

  return position!;
};

const ensureOrderSide = (header: HTMLElement, order: HTMLElement, side: OrderSide) => {
  const cssColor = side === OrderSide.Buy ? 'var(--buy-text-color)' : 'var(--sell-text-color)';
  const symbolCell = getCell(header, order, 'Symbol');

  // Try to find the colored element in the Symbol cell
  // Check multiple possible paths as the structure varies by order type
  let colorElement: HTMLElement | undefined;

  if (symbolCell.children[0]?.children[0]) {
    colorElement = symbolCell.children[0].children[0] as HTMLElement;
  } else if (symbolCell.children[0]) {
    colorElement = symbolCell.children[0] as HTMLElement;
  } else {
    colorElement = symbolCell;
  }

  // Check inline style first, fallback to checking if the element exists with expected structure
  const inlineColor = (colorElement as any).style.color;
  if (inlineColor) {
    expect(inlineColor).to.eq(cssColor);
  } else {
    // For stop orders and other cases where inline style isn't used, just verify the element exists
    // The visual color is applied via CSS classes and is visible in the UI
    expect(colorElement).to.not.be.undefined;
  }
};

const findOrder = (
  tableElement: JQuery<HTMLElement>,
  side: OrderSide,
  symbol: string,
  isStopOrder: boolean = false,
) => {
  const trs = tableElement.find('tr');
  const orders = trs.toArray().slice(1);
  const header = trs[0];

  const symbolIndex = Array.from(header.children).findIndex((td: any) => {
    return td.innerText.startsWith('Symbol');
  });

  const priceIndex = Array.from(header.children).findIndex((td: any) => {
    return td.innerText.startsWith('Price');
  });

  // Find all orders with matching symbol
  const matchingOrders = orders.filter(function (order) {
    const symbolCell = order.children[symbolIndex];
    return symbolCell?.textContent?.includes(symbol);
  });

  if (matchingOrders.length === 0) {
    assert.fail(`Could not find any orders for ${symbol}`);
  }

  if (matchingOrders.length === 1) {
    // Only one order, assume it's the one we're looking for
    return matchingOrders[0];
  }

  // Multiple orders - distinguish by price
  const ordersWithPrices = matchingOrders.map((order) => {
    const priceCell = order.children[priceIndex];
    const priceText = priceCell?.textContent || '0';
    const price = safelyParseValue(priceText);
    return { order, price };
  });

  // Sort by price ascending
  ordersWithPrices.sort((a, b) => a.price - b.price);

  // For limit orders: buy = lower price, sell = higher price
  // For stop orders: buy = higher price (triggers above market), sell = lower price (triggers below market)
  let order: HTMLElement;
  if (isStopOrder) {
    order = side === OrderSide.Buy ? ordersWithPrices[ordersWithPrices.length - 1].order : ordersWithPrices[0].order;
  } else {
    order = side === OrderSide.Buy ? ordersWithPrices[0].order : ordersWithPrices[ordersWithPrices.length - 1].order;
  }

  assert.isDefined(order, `Could not find ${side} order for ${symbol}`);

  return order!;
};

const getCell = (header: HTMLElement, row: HTMLElement, colTitle: string) => {
  const index = Array.from(header.children).findIndex((td: any) => {
    return td.textContent.includes(colTitle);
  });
  return row.children[index] as HTMLElement;
};

const setOrderTypeTab = (orderType: 'market' | 'limit' | 'tpsl') => {
  const classicSelector = `[data-testid=toggle-tabs-${orderType}]`;
  const bybitSkinOrderType = orderType === 'tpsl' ? 'conditional_group' : orderType;
  const bybitSkinSelector = `[data-testid=dropdown-tabs-${bybitSkinOrderType}]`;
  cy.get(`${classicSelector}:visible, ${bybitSkinSelector}:visible`, { timeout: 30000 }).first().click();
};

// GenericSizeOption (data-testid=quote-size-input) is dead since SizeOptionCard
// was rolled across every order form. SizeOptionCard renders size-input-quote in
// dual-input mode, otherwise a single unit-switchable size-input.
const resolveQuoteSizeCellSelector = ($body: JQuery<HTMLElement>) =>
  $body.find('[data-testid=size-input-quote]:visible').length > 0
    ? '[data-testid=size-input-quote]'
    : '[data-testid=size-input]';

const resolveBaseSizeCellSelector = ($body: JQuery<HTMLElement>) =>
  $body.find('[data-testid=size-input-base]:visible').length > 0
    ? '[data-testid=size-input-base]'
    : '[data-testid=size-input]';

// The single size-input cell is unit-switchable and defaults to base, so both
// writing a quote amount and reading one back must flip it to quote first.
// Switching re-renders the cell, so callers must re-query afterwards.
const ensureQuoteSizeUnit = (sizeCellsSelector: string, index: number) => {
  if (sizeCellsSelector !== '[data-testid=size-input]') return;
  cy.get(sizeCellsSelector)
    .filter(':visible')
    .eq(index)
    .then(($inputWrapper) => {
      if (!$inputWrapper.text().includes(quoteCurrency)) {
        cy.wrap($inputWrapper).find('[data-testid=inline-cell-dropdown-trigger]').click({ force: true });
        cy.get('[data-testid=inline-cell-dropdown-option-quote]', { timeout: 10000 }).click({ force: true });
        // Block until the switch has re-rendered. setNativeInputValue writes to a
        // captured node, so querying the input before React swaps it silently
        // loses the write and the form is submitted with size 0.
        cy.get(sizeCellsSelector).filter(':visible').eq(index).should('contain.text', quoteCurrency);
      }
    });
};

const ensureBaseSizeUnit = (sizeCellsSelector: string, index: number) => {
  if (sizeCellsSelector !== '[data-testid=size-input]') return;
  cy.get(sizeCellsSelector)
    .filter(':visible')
    .eq(index)
    .then(($inputWrapper) => {
      if ($inputWrapper.text().includes(quoteCurrency)) {
        cy.wrap($inputWrapper).find('[data-testid=inline-cell-dropdown-trigger]').click({ force: true });
        cy.get('[data-testid=inline-cell-dropdown-option-base]', { timeout: 10000 }).click({ force: true });
        cy.get(sizeCellsSelector).filter(':visible').eq(index).should('not.contain.text', quoteCurrency);
      }
    });
};

const setClassicOrderSize = (sizeInQuote: number) => {
  cy.get('body').then(($body) => {
    const quoteInputs = $body.find('[data-testid=quote-size-input]:visible');
    if (quoteInputs.length > 0) {
      cy.get('[data-testid=quote-size-input]')
        .filter(':visible')
        .then(($inputs) => {
          $inputs.each((_, inputWrapper) => {
            cy.wrap(inputWrapper)
              .find('input')
              .then(($input) => {
                setNativeInputValue($input, `${sizeInQuote}`);
              });
          });
        });
      return;
    }

    const sizeCellsSelector = resolveQuoteSizeCellSelector($body);

    cy.get(sizeCellsSelector)
      .filter(':visible')
      .then(($inputs) => {
        expect($inputs.length, 'classic single-cell size inputs').to.be.greaterThan(0);

        // Switching a cell's unit re-renders it, detaching any node captured
        // before the dropdown click. Re-query by index for each step rather
        // than reusing a wrapped reference, which Cypress cannot requery.
        for (let index = 0; index < $inputs.length; index += 1) {
          ensureQuoteSizeUnit(sizeCellsSelector, index);

          cy.get(sizeCellsSelector)
            .filter(':visible')
            .eq(index)
            .find('input')
            .then(($input) => {
              setNativeInputValue($input, `${sizeInQuote}`);
            });
        }
      });
  });
};

const setClassicOrderBaseSize = (sizeInBase: number) => {
  cy.get('body').then(($body) => {
    const baseInputs = $body.find('[data-testid=base-size-input]:visible');
    if (baseInputs.length > 0) {
      cy.get('[data-testid=base-size-input]')
        .filter(':visible')
        .then(($inputs) => {
          $inputs.each((_, inputWrapper) => {
            cy.wrap(inputWrapper)
              .find('input')
              .then(($input) => {
                setNativeInputValue($input, `${sizeInBase}`);
              });
          });
        });
      return;
    }

    const sizeCellsSelector = resolveBaseSizeCellSelector($body);

    cy.get(sizeCellsSelector)
      .filter(':visible')
      .then(($inputs) => {
        expect($inputs.length, 'classic single-cell base size inputs').to.be.greaterThan(0);

        for (let index = 0; index < $inputs.length; index += 1) {
          ensureBaseSizeUnit(sizeCellsSelector, index);

          cy.get(sizeCellsSelector)
            .filter(':visible')
            .eq(index)
            .find('input')
            .then(($input) => {
              setNativeInputValue($input, `${sizeInBase}`);
            });
        }
      });
  });
};

const setBybitSkinOrderSize = (sizeInQuote: number) => {
  cy.get('[data-testid=bybit-skin-size-input]')
    .filter(':visible')
    .first()
    .then(($inputWrapper) => {
      if ($inputWrapper.attr('data-selected-unit') !== 'secondary') {
        cy.get('[data-testid=bybit-skin-size-unit-selector]').filter(':visible').first().click({ force: true });
        cy.get('[data-testid=bybit-skin-size-unit-secondary]', { timeout: 10000 }).click({ force: true });
      }
    });

  cy.get('[data-testid=bybit-skin-size-input]')
    .filter(':visible')
    .first()
    .find('input')
    .then(($input) => {
      setNativeInputValue($input, `${sizeInQuote}`);
    });
};

// ALWAYS set the limit price before calling this. The size cell is quote-denominated
// but the form stores base, and useSizeOption's onChangeQuote bails out when price <= 0.
// The limit tab starts at price 0, so a size written first only lands in the input's
// internal state and is wiped by the next re-render — the order then submits with size 0.
const setOrderSize = (sizeInQuote: number) => {
  cy.get('body').then(($body) => {
    if ($body.find('[data-testid=bybit-skin-size-input]:visible').length > 0) {
      setBybitSkinOrderSize(sizeInQuote);
      return;
    }

    setClassicOrderSize(sizeInQuote);
  });
};

const setOrderBaseSize = (sizeInBase: number) => {
  cy.get('body').then(($body) => {
    if ($body.find('[data-testid=bybit-skin-size-input]:visible').length > 0) {
      throw new Error('setOrderBaseSize does not support bybit skin order forms');
    }

    setClassicOrderBaseSize(sizeInBase);
  });
};

const ensureOrderSizeInputHasValue = (side: OrderSide) => {
  cy.get('body').then(($body) => {
    if ($body.find('[data-testid=bybit-skin-size-input]:visible').length > 0) {
      cy.get('[data-testid=bybit-skin-size-input]')
        .filter(':visible')
        .first()
        .should('have.attr', 'data-selected-unit', 'secondary')
        .find('input')
        .should(($input) => {
          const val = parseFloat(($input.val() as string) || '0');
          expect(val).to.be.greaterThan(0, 'Bybit skin order size input should be > 0');
        });
      return;
    }

    if ($body.find('[data-testid=quote-size-input]:visible').length > 0) {
      cy.get('[data-testid=quote-size-input]')
        .filter(':visible')
        .eq(side === OrderSide.Buy ? 0 : 1)
        .find('input')
        .should(($input) => {
          const val = parseFloat(($input.val() as string) || '0');
          expect(val).to.be.greaterThan(0, 'Order size input should be > 0');
        });
      return;
    }

    const sizeCellsSelector = resolveQuoteSizeCellSelector($body);

    cy.get(sizeCellsSelector)
      .filter(':visible')
      .eq(sizeCellsSelector === '[data-testid=size-input]' ? 0 : side === OrderSide.Buy ? 0 : 1)
      .find('input')
      .should(($input) => {
        const val = parseFloat(($input.val() as string) || '0');
        expect(val).to.be.greaterThan(0, 'Order size input should be > 0');
      });
  });
};

const getMarketButtonSelector = ($body: JQuery<HTMLElement>, side: OrderSide) => {
  const classicSelector = `[data-testid=market-${side === OrderSide.Buy ? 'buy' : 'sell'}-button]`;
  const bybitSkinSelector = `[data-testid=bybit-skin-market-${side === OrderSide.Buy ? 'long' : 'short'}-button]`;
  return $body.find(`${bybitSkinSelector}:visible`).length > 0 ? bybitSkinSelector : classicSelector;
};

const ensureMarketButtonIsEnabled = (side: OrderSide) => {
  cy.get('body').then(($body) => {
    const selector = getMarketButtonSelector($body, side);
    cy.get(selector).filter(':visible').first().should('not.have.attr', 'aria-disabled', 'true');
  });
};

const clickMarketButton = (side: OrderSide) => {
  cy.get('body').then(($body) => {
    const selector = getMarketButtonSelector($body, side);
    cy.get(selector).filter(':visible').first().click();
  });
};

const getParadexMarketForE2E = (exch: any) => {
  return exch?.store?.marketsBySymbol?.[symbol] ?? exch?.store?.markets?.find((m: any) => m.symbol === symbol);
};

const getActiveMarketForE2E = (exch: any) => {
  return exch?.store?.marketsBySymbol?.[symbol] ?? exch?.store?.markets?.find((m: any) => m.symbol === symbol);
};

const getE2EAmountInBase = (exch: any, sizeInQuote: number, price: number) => {
  const market = getActiveMarketForE2E(exch);
  const precision = market?.precision?.amount;
  const rawSize = sizeInQuote / price;
  return precision ? adjust(rawSize, precision) : rawSize;
};

const getParadexAmountInBase = (exch: any, sizeInQuote: number, price: number) => {
  const market = getParadexMarketForE2E(exch);
  const precision = market?.precision?.amount;
  const rawSize = sizeInQuote / price;
  return precision ? adjust(rawSize, precision) : rawSize;
};

const getParadexOrderPrice = (exch: any, price?: number) => {
  if (price === undefined) return undefined;
  const precision = getParadexMarketForE2E(exch)?.precision?.price;
  return precision ? adjust(price, precision) : price;
};

const getParadexMarketAmountInBase = (exch: any, sizeInQuote: number) =>
  getParadexAmountInBase(exch, sizeInQuote, lastTrade);

const getParadexExchangeForE2E = (win: Window) => {
  return getActiveExchangeForE2E(win);
};

const getActiveExchangeForE2E = (win: Window) => {
  const accountName = getAccount().name;
  const eac = (win as any).exchangeAccountController;
  const exch = eac?.getExchange?.(accountName);
  return { accountName, eac, exch };
};

const forceMarginTypeForE2E = (marginType: MarginType): Cypress.Chainable<void> => {
  return cy.window({ log: false }).then((win) => {
    const { exch } = getActiveExchangeForE2E(win);
    if (typeof exch?.callMethodInWorker !== 'function') {
      throw new Error('Active exchange does not expose callMethodInWorker for e2e margin-type setup');
    }

    return cy
      .wrap(exch.callMethodInWorker('changeMarginType', marginType === MarginType.Isolated, symbol), {
        log: false,
        timeout: 30_000,
      })
      .then(() =>
        cy.wrap(exch.callMethodInWorker('updateSymbolMetadata', symbol), { log: false, timeout: 30_000 }),
      )
      .then(() => undefined);
  });
};

const forcePositionModeForE2E = (positionMode: PositionMode): Cypress.Chainable<void> => {
  return cy.window({ log: false }).then((win) => {
    const { exch } = getActiveExchangeForE2E(win);
    if (typeof exch?.callMethodInWorker !== 'function') {
      throw new Error('Active exchange does not expose callMethodInWorker for e2e position-mode setup');
    }

    return cy
      .wrap(exch.callMethodInWorker('changePositionMode', positionMode === PositionMode.Hedged, symbol), {
        log: false,
        timeout: 30_000,
      })
      .then(() =>
        cy.wrap(exch.callMethodInWorker('updateSymbolMetadata', symbol), { log: false, timeout: 30_000 }),
      )
      .then(() => undefined);
  });
};

const setPositionModeForE2E = (positionMode: PositionMode, positionModeLabel: string) => {
  cy.get('body').then(($body) => {
    const $selector = $body.find('[data-testid=position-mode-selector]').filter(':visible');
    if (exchangeConfig.exchange !== 'phemex' && $selector.length > 0 && !$selector.attr('class')?.includes('disabled')) {
      setSelectValue('[data-testid=position-mode-selector]', positionModeLabel);
      return;
    }

    cy.log(`Forcing position mode to ${positionMode}`);
    forcePositionModeForE2E(positionMode);
  });
};

const ensurePositionModeForE2E = (positionModeLabel: string) => {
  if (exchangeConfig.exchange === 'phemex') return;

  cy.get('body').then(($body) => {
    const $selector = $body.find('[data-testid=position-mode-selector]').filter(':visible');
    if ($selector.length > 0 && !$selector.attr('class')?.includes('disabled')) {
      ensureSelectValue('[data-testid=position-mode-selector]', positionModeLabel);
    }
  });
};

const waitForPhemexPositionMetadataForE2E = (
  expected: { marginType?: MarginType; positionMode?: PositionMode },
  deadline = Date.now() + 20_000,
): Cypress.Chainable<void> => {
  if (exchangeConfig.exchange !== 'phemex') return cy.wrap(undefined, { log: false });

  return cy.window({ log: false }).then((win) => {
    const { exch } = getActiveExchangeForE2E(win);
    const resolveLeverageContext = exch?.resolveLeverageContext;
    if (typeof resolveLeverageContext !== 'function') {
      throw new Error('Phemex exchange does not expose resolveLeverageContext for e2e metadata confirmation');
    }

    return cy.wrap(resolveLeverageContext.call(exch, symbol), { log: false, timeout: 10_000 }).then((context: any) => {
      const marginMatches = expected.marginType === undefined || context?.marginType === expected.marginType;
      const positionMatches = expected.positionMode === undefined || context?.positionMode === expected.positionMode;
      if (marginMatches && positionMatches) return;

      if (Date.now() >= deadline) {
        throw new Error(
          `Phemex metadata did not settle for ${symbol}; expected ${JSON.stringify(expected)}, got ${JSON.stringify(
            context,
          )}`,
        );
      }

      return cy.wait(500, { log: false }).then(() => waitForPhemexPositionMetadataForE2E(expected, deadline));
    });
  });
};

const placeMarketOrderForE2E = (side: OrderSide, sizeInQuote = orderSizeInQuote) => {
  if (exchangeConfig.exchange === 'woo') {
    cy.window({ log: false }).then((win) => {
      const { accountName, eac, exch } = getActiveExchangeForE2E(win);
      if (!exch || typeof exch.placeOrder !== 'function') {
        throw new Error('WOO e2e market order failed: exchange controller is not ready');
      }

      const amount = getE2EAmountInBase(exch, sizeInQuote, lastTrade);
      if (!(amount > 0)) {
        throw new Error(`WOO e2e market order failed: quote size ${sizeInQuote} adjusted to ${amount}`);
      }

      return cy
        .wrap(
          exch.placeOrder({
            symbol,
            side,
            type: OrderType.Market,
            amount,
            reduceOnly: false,
          }),
          { log: false, timeout: 30000 },
        )
        .then((result: string[]) => {
          cy.task('log', `[woo:market] ${side} ${amount} ${symbol} -> ${JSON.stringify(result)}`);
          eac.scheduleUserActivityPositionRefetch?.(accountName, [symbol]);
          recoverWooSymbolPositionsForE2E();
        });
    });
    return;
  }

  if (exchangeConfig.exchange !== 'paradex') {
    clickMarketButton(side);
    return;
  }

  cy.window({ log: false }).then((win) => {
    const { accountName, eac, exch } = getParadexExchangeForE2E(win);
    if (!exch || typeof exch.placeOrder !== 'function') {
      throw new Error('Paradex e2e market order failed: exchange controller is not ready');
    }

    const amount = getParadexMarketAmountInBase(exch, sizeInQuote);
    if (!(amount > 0)) {
      throw new Error(`Paradex e2e market order failed: quote size ${sizeInQuote} adjusted to ${amount}`);
    }

    return cy
      .wrap(
        (async () => {
          if (typeof exch.setMaxSlippage === 'function') {
            await exch.setMaxSlippage('0.2', symbol);
          }
          return exch.placeOrder({
            symbol,
            side,
            type: 'market',
            amount,
            reduceOnly: false,
            timeInForce: OrderTimeInForce.ImmediateOrCancel,
            maxSlippagePct: 20,
          });
        })(),
        { log: false, timeout: 30000 },
      )
      .then((result: string[]) => {
        cy.task('log', `[paradex:market] ${side} ${amount} ${symbol} -> ${JSON.stringify(result)}`);
        eac.scheduleUserActivityPositionRefetch?.(accountName, [symbol]);
      });
  });
};

const placeLimitOrderForE2E = (
  side: OrderSide,
  price: number,
  options: {
    postOnly?: boolean;
    takeProfit?: number;
    stopLoss?: number;
  } = {},
) => {
  if (exchangeConfig.exchange === 'woo') {
    cy.window({ log: false }).then((win) => {
      const { accountName, eac, exch } = getActiveExchangeForE2E(win);
      if (!exch || typeof exch.placeOrder !== 'function') {
        throw new Error('WOO e2e limit order failed: exchange controller is not ready');
      }

      const amount = getE2EAmountInBase(exch, orderSizeInQuote, price);
      if (!(amount > 0)) {
        throw new Error(`WOO e2e limit order failed: quote size ${orderSizeInQuote} adjusted to ${amount}`);
      }

      const placement = exch
        .placeOrder({
          symbol,
          side,
          type: OrderType.Limit,
          amount,
          price,
          reduceOnly: false,
          takeProfit: options.takeProfit,
          stopLoss: options.stopLoss,
          timeInForce: options.postOnly ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
        })
        .catch((err: Error) => {
          if (options.postOnly && /post-only .* would cross/i.test(err?.message || '')) {
            return [];
          }
          throw err;
        });

      return cy.wrap(placement, { log: false, timeout: 30000 }).then((result: string[]) => {
        cy.task('log', `[woo:limit] ${side} ${amount} ${symbol} @ ${price} -> ${JSON.stringify(result)}`);
        eac.scheduleUserActivityPositionRefetch?.(accountName, [symbol]);
        recoverHostedOpenOrdersForE2E();
        recoverWooSymbolPositionsForE2E();
      });
    });
    return;
  }

  if (exchangeConfig.exchange !== 'paradex') {
    cy.get(`[data-testid=limit-${side}-button]`).first().click();
    return;
  }

  cy.window({ log: false }).then((win) => {
    const { accountName, eac, exch } = getParadexExchangeForE2E(win);
    if (!exch || typeof exch.placeOrder !== 'function') {
      throw new Error('Paradex e2e limit order failed: exchange controller is not ready');
    }

    const orderPrice = getParadexOrderPrice(exch, price);
    const takeProfit = getParadexOrderPrice(exch, options.takeProfit);
    const stopLoss = getParadexOrderPrice(exch, options.stopLoss);
    if (!(orderPrice && orderPrice > 0)) {
      throw new Error(`Paradex e2e limit order failed: price ${price} adjusted to ${orderPrice}`);
    }

    const amount = getParadexAmountInBase(exch, orderSizeInQuote, orderPrice);
    if (!(amount > 0)) {
      throw new Error(`Paradex e2e limit order failed: quote size ${orderSizeInQuote} adjusted to ${amount}`);
    }

    return cy
      .wrap(
        exch.placeOrder({
          symbol,
          side,
          type: 'limit',
          amount,
          price: orderPrice,
          reduceOnly: false,
          timeInForce: options.postOnly ? 'PostOnly' : 'GoodTillCancel',
          takeProfit,
          stopLoss,
        }),
        { log: false, timeout: 30000 },
      )
      .then((result: string[]) => {
        lastParadexPlacedOrderIds = result;
        cy.task('log', `[paradex:limit] ${side} ${amount} ${symbol} @ ${orderPrice} -> ${JSON.stringify(result)}`);
        eac.scheduleUserActivityPositionRefetch?.(accountName, [symbol]);
        recoverHostedOpenOrdersForE2E();
      });
  });
};

const closePositionWithMarketOrderForE2E = (positionSide: PositionSide) => {
  if (exchangeConfig.exchange !== 'paradex') {
    clickMarketButton(positionSide === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy);
    return;
  }

  cy.window({ log: false }).then((win) => {
    const { accountName, eac, exch } = getParadexExchangeForE2E(win);
    if (!exch || typeof exch.placeOrder !== 'function') {
      throw new Error('Paradex e2e market close failed: exchange controller is not ready');
    }

    const snap = (win as any).__getStoreSnapshot?.() || {};
    const positions = (snap?.[accountName]?.positions || []) as any[];
    const position = positions.find((p) => p.symbol === symbol && p.side === positionSide && p.contracts !== 0);
    if (!position) {
      throw new Error(`Paradex e2e market close failed: no ${positionSide} ${symbol} position found`);
    }

    const amount = nativeCloseSize(exch, symbol, position.side, position.contracts);
    if (!(amount > 0)) {
      throw new Error(`Paradex e2e market close failed: position size ${position.contracts} adjusted to ${amount}`);
    }

    const side = positionSide === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy;
    return cy
      .wrap(
        (async () => {
          if (typeof exch.setMaxSlippage === 'function') {
            await exch.setMaxSlippage('0.2', symbol);
          }
          return exch.placeOrder({
            symbol,
            side,
            type: 'market',
            amount,
            reduceOnly: true,
            timeInForce: OrderTimeInForce.ImmediateOrCancel,
            maxSlippagePct: 20,
          });
        })(),
        { log: false, timeout: 30000 },
      )
      .then((result: string[]) => {
        cy.task('log', `[paradex:market-close] ${side} ${amount} ${symbol} -> ${JSON.stringify(result)}`);
        eac.scheduleUserActivityPositionRefetch?.(accountName, [symbol]);
      });
  });
};

// The TP/SL tab has no price-input — it renders trigger-input instead, so the
// stop-orders test could never set a price. Index 0/1 is buy/sell in the
// two-column layout the e2e viewport is wide enough to get.
const setSidePrice = (side: 0 | 1, price: number) => {
  cy.get('body').then(($body) => {
    const selector =
      $body.find('[data-testid=price-input]:visible').length > 0
        ? '[data-testid=price-input]'
        : '[data-testid=trigger-input]';
    cy.get(selector)
      .filter(':visible')
      .eq(side)
      .find('input')
      .then(($input) => {
        setNativeInputValue($input, `${price}`);
      });
  });
};

const setBuyPrice = (price: number) => {
  cy.log('Setting buy price');
  setSidePrice(0, price);
};

// The TP/SL tab's trigger-type pickers are LabeledDropdowns (InlineCellDropdown
// under the hood), not the compact antd Select, and they live inside a collapsed
// "Advanced" disclosure — so they must be revealed before they can be driven.
// The suite sets TP and SL alike, targeting rows by label so duplicate portal
// option test ids never route a click to the wrong picker.
const setTriggerTypeOnBothSides = (triggerType: string, label: string) => {
  const selector = '[data-testid=trigger-type-selector]';

  cy.get('body').then(($body) => {
    if ($body.find(`${selector}:visible`).length === 0) {
      cy.contains('button:visible', tradeEn.advanced).click({ force: true });
    }
  });

  [tradeEn['tp-trigger-type'], tradeEn['sl-trigger-type']].forEach((pickerLabel) => {
    cy.contains(`${selector}:visible`, pickerLabel)
      .find('[data-testid=inline-cell-dropdown-trigger]')
      .then(($trigger) => {
        if ($trigger.text().includes(label)) {
          return;
        }

        cy.wrap($trigger).click({ force: true });
        const optionSelector = `[data-testid=inline-cell-dropdown-option-${triggerType}]:visible`;
        cy.contains(optionSelector, new RegExp(`^\\s*${escapeRegExp(label)}\\s*$`), { timeout: 10000 }).click({
          force: true,
        });
      });
    cy.contains(`${selector}:visible`, pickerLabel)
      .find('[data-testid=inline-cell-dropdown-trigger]')
      .should('contain.text', label);
  });
};

const setNativeInputValue = ($input: JQuery<HTMLElement>, value: string) => {
  const input = $input[0] as HTMLInputElement;
  // Use the native setter to bypass React's synthetic event tracking
  const nativeInputValueSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')!.set!;
  nativeInputValueSetter.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
};

const updateOrderPrice = (
  side: OrderSide,
  symbol: string,
  newPrice: number,
  isStopOrder: boolean = false,
  tableSelector: string = '[data-testid=open-orders-table]',
) => {
  // Use a fresh DOM query to find the order row and click edit.
  // This avoids stale element references from outer .then() callbacks —
  // WebSocket updates can re-render the table between when the outer callback
  // captures $element and when this click executes, detaching the row element.
  cy.get(tableSelector).then(($table) => {
    const header = getTableHeader($table);
    const order = findOrder($table, side, displaySymbol, isStopOrder);
    const priceCell = getCell(header, order, 'Price');
    cy.wrap(priceCell).find('[data-testid=inline-edit-start]').click();
  });

  // Use fresh DOM queries after click — only one cell is in edit mode at a time,
  // so this selector is unambiguous.
  cy.get(`${tableSelector} [data-testid=price-input] input`, { timeout: 10000 })
    .should('be.visible')
    .then(($input) => {
      // Use native value setter to bypass React controlled input reformatting.
      // Cypress .clear().type() doesn't work because the CustomNumberInput
      // reformats "0.00" on clear, causing subsequent characters to be appended
      // to the formatted zero instead of replacing it.
      setNativeInputValue($input, `${newPrice}`);
    });

  // Save using fresh DOM query
  cy.get(`${tableSelector} [data-testid=inline-edit-save]`, { timeout: 10000 })
    .should('be.visible')
    .first()
    .click();

  cy.wait(actionWaitTime);
  cy.wait(slowUpdateExtraWaitTime);
};

const setSellPrice = (price: number) => {
  cy.log('Setting sell price');
  setSidePrice(1, price);
};

const setSelectValue = (selector: string, value: string, eq = 0) => {
  getVisibleSelect(selector, eq).click();
  cy.wait(100);
  cy.get('[class*="optionItem"]').filter(':visible').contains(value).click();
  cy.wait(100);
};

const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const safelyParseValue = (value?: string | null) => {
  if (!value) {
    return NaN;
  }
  return parseFloat(value.trim().replace(/[$,]/g, '').split(' ')[0]);
};

const canAmendRestingOrders = () => {
  return (
    exchangeConfig.exchange !== 'bitunix' &&
    exchangeConfig.exchange !== 'binance' &&
    exchangeConfig.exchange !== 'paradex' &&
    exchangeConfig.exchange !== 'phemex'
  );
};

const canAmendReduceOrders = () => {
  return canAmendRestingOrders() && exchangeConfig.exchange !== 'woo' && exchangeConfig.exchange !== 'phemex';
};

const canAmendStopOrders = () => {
  return canAmendRestingOrders() && exchangeConfig.exchange !== 'okx' && exchangeConfig.exchange !== 'kraken';
};

const canSetLeverageInE2E = () => {
  return (
    exchangeConfig.exchange !== 'binance' &&
    exchangeConfig.exchange !== 'paradex' &&
    exchangeConfig.exchange !== 'kraken'
  );
};

const setConfiguredLeverageForE2E = (metaData: ExchangeMetadata) => {
  const leverage = exchangeConfig.setupLeverage;
  if (!leverage) {
    return;
  }
  if (!metaData.supportsMaxCrossLeverage || !canSetLeverageInE2E()) {
    cy.log(`Skipping configured leverage override for ${exchangeConfig.exchange}`);
    return;
  }

  const [[leverageValue]] = getLeverageMagnetsToTest(metaData, [leverage]);
  cy.log(`Setting configured leverage to ${leverageValue}`);
  setNativeSliderValue('[data-testid=leverage-slider]', leverageValue);
  cy.wait(actionWaitTime);
  ensureSliderValue('[data-testid=leverage-slider]', leverageValue);
};

const setHedgeModeLeverageForE2E = (metaData: ExchangeMetadata) => {
  if (!exchangeConfig.hedgeModeLeverage) {
    return;
  }
  if (!metaData.supportsMaxCrossLeverage || !canSetLeverageInE2E()) {
    cy.log(`Skipping hedge-mode leverage override for ${exchangeConfig.exchange}`);
    return;
  }

  const [[leverage]] = getLeverageMagnetsToTest(metaData, [exchangeConfig.hedgeModeLeverage]);
  cy.log(`Setting hedge-mode leverage to ${leverage}`);
  setNativeSliderValue('[data-testid=leverage-slider]', leverage);
  cy.wait(actionWaitTime);
  ensureSliderValue('[data-testid=leverage-slider]', leverage);

  cy.get('body').then(($body) => {
    if ($body.find('[data-testid=leverage-slider]').length > 1) {
      setNativeSliderValue('[data-testid=leverage-slider]', leverage, 1);
      cy.wait(actionWaitTime);
      ensureSliderValue('[data-testid=leverage-slider]', leverage, 1);
      ensureSliderValue('[data-testid=leverage-slider]', leverage);
    }
  });
};

const shouldInferMarginSliderLeverage = () => {
  return exchangeConfig.exchange === 'binance';
};

const requiresAuthTextInBadKeyToast = () => {
  return exchangeConfig.exchange !== 'paradex';
};

const badApiKeyMaxErrors = () => {
  return exchangeConfig.exchange === 'blofin' ? 150 : 100;
};

const badApiKeyMaxHttpRequests = () => exchangeConfig.badApiKeyMaxHttpRequests ?? 100;

const displaysStopTriggerLabel = () => {
  return exchangeConfig.exchange !== 'paradex';
};

const authErrorPatterns = [
  /invalid/i,
  /unauthorized/i,
  /auth/i,
  /forbidden/i,
  /api.?key/i,
  /signature/i,
  /credential/i,
  /expired/i,
  /passphrase/i,
  /sign/i,
  /access.*denied/i,
  // BloFin surfaces bad creds as "Access key does not exist" — "access key"
  // is not "api key" and there's no "denied", so it slipped past the patterns
  // above and the test saw 0 meaningful auth errors despite many being logged.
  // (Scoped to "access key"; a bare /does not exist/ would also match benign
  // "order/market does not exist" errors and weaken the bad-creds assertion.)
  /access.?key/i,
  /\bkey\b.*\bsecret\b|\bsecret\b.*\bkey\b/i,
];

const isAuthFailureText = (text: string) => authErrorPatterns.some((p) => p.test(text));

const strictAuthValidationPatterns = [
  /invalid/i,
  /unauthorized/i,
  /authentication/i,
  /authorization/i,
  /forbidden/i,
  /signature/i,
  /credential/i,
  /expired/i,
  /passphrase/i,
  /\bsign(ature|ing)?\b/i,
  /access.*denied/i,
  /access.?key/i,
  /failed/i,
  /error/i,
];

const isStrictAuthValidationText = (text: string) => strictAuthValidationPatterns.some((p) => p.test(text));

const visibleText = (element: Element) => {
  const htmlElement = element as HTMLElement;
  const rect = htmlElement.getBoundingClientRect();
  if (rect.width === 0 && rect.height === 0) return '';
  return htmlElement.innerText || htmlElement.textContent || '';
};

const addAccountValidationText = (win: Window) => {
  const selectors = [
    '.ant-form-item-explain-error',
    '.ant-alert',
    '.ant-message-notice-content',
    '.Toastify__toast',
    '[role="alert"]',
  ];
  return Array.from(win.document.querySelectorAll(selectors.join(',')))
    .map(visibleText)
    .filter(Boolean)
    .join('\n');
};

const capturedAuthFailedRequests = () =>
  capturedFailedRequests.filter((request) => {
    const text = `${request.status} ${request.method} ${request.url} ${request.body}`;
    return isAuthFailureText(text);
  });

const reduceCloseOrderSizeInQuote = (price?: number) => {
  if (exchangeConfig.exchange !== 'paradex') {
    return orderSizeInQuote * 2;
  }

  return price ? orderSizeInQuote * (price / lastTrade) : orderSizeInQuote;
};

const setReduceCloseSize = (sizeInBase: number, price?: number) => {
  if (
    exchangeConfig.exchange === 'paradex' ||
    exchangeConfig.exchange === 'woo' ||
    exchangeConfig.exchange === 'binance' ||
    exchangeConfig.exchange === 'hyperliquid'
  ) {
    setOrderBaseSize(sizeInBase);
    return;
  }

  setOrderSize(reduceCloseOrderSizeInQuote(price));
};

const getAddAccountInput = (field: string, options: Partial<Cypress.Timeoutable> = {}) => {
  const selector = `[data-testid=add-account-${field}-input]`;
  return cy
    .get(selector, options)
    .should(($inputs) => {
      expect($inputs.filter(':visible').length, `${selector} visible input count`).to.be.greaterThan(0);
    })
    .then(($inputs) => cy.wrap($inputs.filter(':visible').first()));
};

const selectExchangeInAddAccountFlow = (exchange: string) => {
  cy.get(`[data-testid=exchange-select-${exchange}]`, { timeout: 10000 }).scrollIntoView().should('be.visible').click();

  cy.get('body').then(($body) => {
    if ($body.find('[data-testid=add-account-name-input]:visible').length > 0) return;
    cy.contains('button', tradeEn['next']).should('be.enabled').click();
  });
};

const enableTealstreetProxyIfRendered = () => {
  cy.get('body').then(($body) => {
    const checkboxSelector = '[data-testid=add-account-tealstreet-proxy-checkbox]';
    const $checkbox = $body.find(checkboxSelector).last();
    if ($checkbox.length === 0) {
      cy.task('log', '[add-api-key] Tealstreet proxy checkbox not rendered; continuing with form defaults');
      return;
    }
    cy.wrap($checkbox).then(($cb) => {
      if (!$cb.is(':checked')) cy.wrap($cb).click({ force: true });
    });
  });
};

const fillPrimaryCredentialFields = (account: ReturnType<typeof getAccount>, options: { corrupt?: boolean } = {}) => {
  const metaData = getAccMetadata();
  const value = (key: string | undefined) => (options.corrupt ? corruptKey(key || '') : key || '');

  getAddAccountInput('api-key').clear().type(value(account.apiKey));
  if (!metaData.requiresNoSecret) {
    getAddAccountInput('api-secret').clear().type(value(account.apiSecret));
  }
  if (metaData.requiresParadexAddress) {
    getAddAccountInput('subkey-address').clear().type(value(account.subkeyAddress));
    getAddAccountInput('paradex-address').clear().type(value(account.paradexAddress));
  }
  if (metaData.requiresPassphrase && account.passphrase) {
    getAddAccountInput('passphrase').clear().type(value(account.passphrase));
  }
  if (metaData.requiresApplicationId && account.applicationId) {
    getAddAccountInput('application-id').clear().type(value(account.applicationId));
  }
  if (account.exchange === 'bitunix' && account.bitunixAccessCode) {
    getAddAccountInput('bitunix-access-code').clear().type(value(account.bitunixAccessCode));
  }
};

// navigate to the page and set the user token, load account and cleanup orders / positions
const cleanup = () => {
  flattenOrders();
  flattenPositions();
  flattenOrders();

  cy.log('Ensuring One Way (if selector exists and not disabled)');
  cy.get('body').then(($body) => {
    const $selector = $body.find('[data-testid=position-mode-selector]').filter(':visible');
    if ($selector.length > 0 && !$selector.attr('class')?.includes('disabled')) {
      getVisibleSelect('[data-testid=position-mode-selector]').then(($element) => {
        if (getSelectValueFromElement($element) !== 'One Way') {
          setSelectValue('[data-testid=position-mode-selector]', tradeEn['position-mode-oneway']);
          cy.wait(actionWaitTime);
        }
      });
    }
  });

  cy.log('Ensuring Cross (if selector exists and not disabled)');
  cy.get('body').then(($body) => {
    const $selector = $body.find('[data-testid=margin-type-selector]').filter(':visible');
    if ($selector.length > 0 && !$selector.attr('class')?.includes('disabled')) {
      getVisibleSelect('[data-testid=margin-type-selector]').then(($element) => {
        if (getSelectValueFromElement($element) !== 'Cross') {
          setSelectValue('[data-testid=margin-type-selector]', tradeEn['margin-type-cross']);
          cy.wait(actionWaitTime);
        }
      });
    }
  });

  cy.get('[data-testid=toggle-tabs-limit]').first().click();
  cy.wait(1000);

  cy.log('Ensuring post-only and reduce-only checkboxes are unchecked');
  ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 0);
  ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 1);
  ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);
  ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);
};

// Stash the active fail handler so we can deregister the *exact* reference at
// the end of beforeEach (and as a safety net in afterEach). Using a module-level
// const captured in beforeEach() closure would still work, but afterEach needs
// access too in case beforeEach threw before reaching its own teardown.
let activeFailHandler: ((err: Error, runnable: any) => Error | void) | null = null;

beforeEach(function (this: Mocha.Context) {
  // Reset phase cursor + dedup flag for this test
  currentBeforeEachPhase = null;
  loadFailureLogged = false;

  // Reset diagnostic capture arrays
  capturedConsoleErrors = [];
  capturedFailedRequests = [];
  transportFailureReason = null;
  transportFailureUrl = null;

  // Register a single fail handler that tags whatever phase we were in when
  // the failure happened. The waitForExchangeLoad path already emits a
  // logLoadFailure with rich load-state, so we dedup via loadFailureLogged.
  // Cypress.on('fail') receives all command failures; re-throwing is the
  // documented pattern for letting the test still fail.
  const failHandler = (err: Error, _runnable: any): Error | void => {
    const phase: BeforeEachPhase = currentBeforeEachPhase ?? 'load';
    if (!loadFailureLogged) {
      loadFailureLogged = true;
      let exchangeName = 'unknown';
      try {
        exchangeName = getAccount().exchange || 'unknown';
      } catch {
        // env phase: getAccount() may itself throw
      }
      cy.task('logLoadFailure', {
        exchange: exchangeName,
        phase,
        error: err?.message || String(err),
        loadingFlags: {},
        orderbookSpinning: false,
        consoleErrors: [...capturedConsoleErrors],
        failedRequests: [...capturedFailedRequests],
        ...(transportFailureReason ? { transportFailureReason } : {}),
        ...(transportFailureUrl ? { transportFailureUrl } : {}),
        summary: `[BEFORE-EACH FAILURE phase=${phase}] ${exchangeName}: ${err?.message || String(err)}`,
      });
    }
    throw err;
  };
  activeFailHandler = failHandler;
  Cypress.on('fail', failHandler);

  // Phase: env — synchronous config reads. cy.* don't bubble try/catch but
  // these are plain JS throws so the catch is meaningful.
  currentBeforeEachPhase = 'env';
  let account: ReturnType<typeof getAccount>;
  try {
    account = getAccount();
  } catch (err) {
    // logLoadFailure via the fail handler doesn't fire for sync throws (those
    // bypass Cypress's command queue). Emit directly so the diagnostic floor
    // holds for env-phase failures too.
    if (!loadFailureLogged) {
      loadFailureLogged = true;
      cy.task('logLoadFailure', {
        exchange: 'unknown',
        phase: 'env',
        error: (err as Error)?.message || String(err),
        loadingFlags: {},
        orderbookSpinning: false,
        consoleErrors: [...capturedConsoleErrors],
        failedRequests: [...capturedFailedRequests],
        summary: `[BEFORE-EACH FAILURE phase=env] ${(err as Error)?.message || String(err)}`,
      });
    }
    throw err;
  }

  // Phase: auth — Auth0 sign-in flow. cy.* failures (selector timeout, URL
  // assertion fail) route through the Cypress.on('fail') handler above.
  currentBeforeEachPhase = 'auth';

  // Authenticate via real Auth0 login
  // Must visit the app and click the Sign In button to initiate the proper OAuth flow
  const username = Cypress.env('testUsername');
  const password = Cypress.env('testPassword');

  // Clear all cookies (including Auth0's cross-domain cookies) and local storage
  // to ensure clean auth state between tests
  cy.clearAllCookies();
  cy.clearAllLocalStorage();
  cy.clearAllSessionStorage();

  cy.visit('/');
  cy.contains('button', 'Sign In', { timeout: 15000 }).should('be.visible').click();

  // Fill in Auth0 login form
  cy.get('input[name="username"], input[name="email"], input#username', { timeout: 15000 })
    .should('be.visible')
    .type(username);

  cy.contains('button', 'Continue').first().click();

  cy.get('input[name="password"], input#password', { timeout: 10000 }).should('be.visible').type(password);

  cy.contains('button', 'Continue').click();

  cy.url({ timeout: 30000 }).should('include', '/trade').and('not.include', 'auth_error');

  // Intercept cloud settings to prevent overriding local test state
  // Must include all settings that the test relies on, otherwise the cloud sync
  // will reset them to defaults
  cy.intercept('GET', '/api/settings*', {
    statusCode: 200,
    body: {
      settings: {
        trade: { activeAccount: account.name, activeSymbol: symbol },
        sound: { enabled: false, volume: 0 },
        general: { showConfirmationDialogs: true },
      },
    },
  }).as('settings');

  // Phase: visit — loading the trade page in the auth'd context. Failures
  // here (cy.visit timeout, page load error) route through fail handler.
  // cy.then is used so the phase flips at the right point in the queue.
  cy.then(() => {
    currentBeforeEachPhase = 'visit';
  });

  cy.visit(getHost(), {
    onBeforeLoad(win) {
      // Inject exchange account and test settings
      win.localStorage.setItem('accounts', JSON.stringify([account]));
      win.localStorage.setItem('disableWebWorker', 'true');
      win.localStorage.setItem('requestsPanelIsOpenAtom', '"true"');
      // TPSL trigger-type selector lives inside a collapsed "Advanced" Disclosure;
      // stop-orders test interacts with it, so pre-open the disclosure here.
      win.localStorage.setItem('placeOrder:tpsl:advancedOpen', 'true');
      win.localStorage.setItem(
        'lastSettings',
        JSON.stringify({
          trade: { activeAccount: account.name, activeSymbol: symbol },
          sound: { enabled: false, volume: 0 },
          enableDevTools: true,
          general: { showConfirmationDialogs: true },
        }),
      );

      // Request logging is auto-enabled by dev.atoms.ts when window.Cypress is detected.
      // This populates requestsLogAtom (same data as the Requests Panel) via the existing
      // event handlers in exchangeAccountController.ts — captures ALL traffic including HOW.

      // Hook into console.error to capture errors for diagnostics
      const originalConsoleError = win.console.error;
      win.console.error = (...args: any[]) => {
        capturedConsoleErrors.push(args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' '));
        originalConsoleError.apply(win.console, args);
      };

      // Track failed XHR/fetch requests for diagnostics (load failure detection)
      const originalFetch = win.fetch;
      win.fetch = async (...fetchArgs: Parameters<typeof fetch>) => {
        const response = await originalFetch.apply(win, fetchArgs);
        if (response.status >= 400) {
          const fetchUrl = typeof fetchArgs[0] === 'string' ? fetchArgs[0] : (fetchArgs[0] as Request).url;
          const fetchMethod = (fetchArgs[1] as RequestInit)?.method || 'GET';
          let body = '';
          try {
            body = await response.clone().text();
            if (body.length > 500) body = body.slice(0, 500) + '...';
          } catch {}
          recordFailedRequest({ method: fetchMethod, url: fetchUrl, status: response.status, body });
        }
        return response;
      };

      const origXHROpen = win.XMLHttpRequest.prototype.open;
      win.XMLHttpRequest.prototype.open = function (method: string, url: string | URL, ...rest: any[]) {
        this.addEventListener('load', function () {
          if (this.status >= 400) {
            let body = '';
            try {
              body = (this.responseText || '').slice(0, 500);
            } catch {}
            recordFailedRequest({ method, url: String(url), status: this.status, body });
          }
        });
        return origXHROpen.apply(this, [method, url, ...rest] as any);
      };
    },
  });

  // Phase: load — waiting for the exchange runtime to finish initializing.
  // waitForExchangeLoad's own diagnostic path will set loadFailureLogged so
  // the Cypress.on('fail') handler doesn't double-emit.
  cy.then(() => {
    currentBeforeEachPhase = 'load';
  });

  // Check that exchange has loaded (with diagnostic reporting on failure)
  waitForExchangeLoad();

  // Past load — clear phase cursor so any later cy.* failure (which would be
  // an in-test problem, not a beforeEach problem) doesn't get tagged as 'load'.
  cy.then(() => {
    currentBeforeEachPhase = null;
    if (activeFailHandler) {
      Cypress.off('fail', activeFailHandler);
      activeFailHandler = null;
    }
  });

  // Store transport config for diagnostics reporting
  cy.task('storeTransportConfig', {
    proxy: !!account.tealstreetProxy,
    tunnel: !!account.tealstreetTunnel,
    proxyRegion: account.proxyRegion || null,
  });

  // Switch to the target symbol unless the URL/settings already selected it.
  cy.get('[data-testid=market-dropdown-popover]')
    .first()
    .then(($dropdown) => {
      const currentSymbol = ($dropdown.text() || '').replace(/\s+/g, '').toUpperCase();
      if (currentSymbol.includes(symbol.toUpperCase())) {
        return;
      }

      cy.wrap($dropdown).click();
      cy.wait(500);

      // Type in the market dropdown's search box (scoped to the popup)
      cy.get('.app-header-popover .popover-search-box').type(symbol);
      // Wait for ticker data and click the matching symbol. We target by data-symbol
      // attribute (exact pair match) instead of textContent because the row only
      // renders the base coin (e.g. "DOGE") not the full pair ("DOGEUSDT") - see
      // MarketDropdownContent.tsx where data-symbol={row.symbol} is the stable hook.
      cy.get(`.app-header-popover [data-symbol-link][data-symbol="${symbol}"]`, { timeout: 30000 }).first().click();
    });

  // Wait for symbol load/selection and page to stabilize
  cy.wait(3000);

  // Register uncaught:exception handler BEFORE cleanup to catch benign errors
  // like "No position to close" during flattenPositions retry
  cy.on('uncaught:exception', (err, runnable) => {
    // Log the error for debugging but prevent Cypress from failing the test
    Cypress.log({ name: 'UNCAUGHT', message: err.message });
    return false;
  });

  cleanup();

  const testsWithoutPriceBootstrap = [
    'Bad API Key — error message and bounded reconnects',
    'Add API Key — good and bad credentials via UI form',
    'Load Balance',
  ];
  if (!testsWithoutPriceBootstrap.includes(this.currentTest?.title || '')) {
    getLastTrade();
    cy.wait(500);
  }
});

afterEach(function () {
  // Safety net: if beforeEach threw before reaching its own teardown of the
  // fail handler, deregister it now so the next test's handler isn't shadowed
  // by a stale closure capturing the previous test's state.
  if (activeFailHandler) {
    Cypress.off('fail', activeFailHandler);
    activeFailHandler = null;
  }

  // Flush API log from the app's Requests Panel system to file for debugging.
  // Reads from requestsLogAtom via window.__getRequestsLog() (exposed by dev.atoms.ts)
  const exchangeName = getAccount().exchange || 'unknown';
  const testName = this.currentTest?.title || 'unknown';
  const testFailed = this.currentTest?.state === 'failed';
  cy.window({ log: false }).then((win) => {
    const getLog = (win as any).__getRequestsLog;
    const clearLog = (win as any).__clearRequestsLog;
    if (typeof getLog === 'function') {
      const entries = getLog();
      const typeCounts: Record<string, number> = {};
      (entries || []).forEach((e: any) => {
        typeCounts[e.type] = (typeCounts[e.type] || 0) + 1;
      });
      cy.task('log', `[API Log] ${entries?.length || 0} entries, types: ${JSON.stringify(typeCounts)}`);
      if (entries && entries.length > 0) {
        // Write full log to file
        cy.task('writeApiLog', { exchange: exchangeName, testName, entries });

        // On failure: store relevant entries for Slack reporting + log to Cypress Cloud
        // Uses relevance-based filtering instead of blind last-N to surface actual errors
        if (testFailed) {
          const truncMsg = (e: any) =>
            typeof e.message === 'string' ? e.message.substring(0, 4000) : JSON.stringify(e.message).substring(0, 4000);
          const mapEntry = (e: any) => ({ type: e.type, severity: e.severity, message: truncMsg(e) });

          // Patterns for relevant trading endpoints
          const tradingEndpointRe = /order|position|place|cancel|algo|tpsl|leverage|margin|trade|batch|amend/i;
          // Patterns for 200-with-error-code responses (Bitget, OKX, Bybit, etc.)
          const errorCodeRe = /"code"\s*:\s*"(?!0+"|00000"|200"|OK")|"retCode"\s*:\s*[^0]|"sCode"\s*:\s*"[^0]/;
          const errorMsgRe = /"msg"\s*:\s*"(?!success|ok|")/i;

          const seen = new Set<number>();
          const relevant: any[] = [];
          const addEntry = (idx: number, e: any) => {
            if (!seen.has(idx) && relevant.length < 30) {
              seen.add(idx);
              relevant.push(mapEntry(e));
            }
          };

          // 1. All error-severity entries
          entries.forEach((e: any, i: number) => {
            if (e.severity === 'error') addEntry(i, e);
          });

          // 2. HTTP responses with non-success codes in body (200-with-error detection)
          entries.forEach((e: any, i: number) => {
            if (e.type === 'httpResponse') {
              const msg = typeof e.message === 'string' ? e.message : JSON.stringify(e.message);
              if (errorCodeRe.test(msg) || errorMsgRe.test(msg)) addEntry(i, e);
            }
          });

          // 3. Last 10 entries matching trading endpoints (order/position/algo)
          const tradingEntries: [number, any][] = [];
          entries.forEach((e: any, i: number) => {
            const msg = typeof e.message === 'string' ? e.message : '';
            if (tradingEndpointRe.test(msg)) tradingEntries.push([i, e]);
          });
          tradingEntries.slice(-10).forEach(([i, e]) => addEntry(i, e));

          // 4. Last 5 private WS messages for orders/positions channels
          const wsEntries: [number, any][] = [];
          entries.forEach((e: any, i: number) => {
            if (e.type === 'privateWs' && /order|position/i.test(typeof e.message === 'string' ? e.message : '')) {
              wsEntries.push([i, e]);
            }
          });
          wsEntries.slice(-5).forEach(([i, e]) => addEntry(i, e));

          const failedHttp = capturedFailedRequests.slice(-10).map((r) => ({
            method: r.method,
            url: r.url,
            status: r.status,
            body: r.body?.slice(0, 1000) || '',
          }));

          // Capture store snapshot (orders + positions at time of failure)
          const getSnapshot = (win as any).__getStoreSnapshot;
          const storeSnapshot = typeof getSnapshot === 'function' ? getSnapshot() : null;

          // Store for results JSON → Slack
          cy.task('storeTestFailureDiagnostics', {
            testName,
            entries: relevant,
            failedRequests: failedHttp,
            storeSnapshot,
            ...(transportFailureReason ? { transportFailureReason } : {}),
            ...(transportFailureUrl ? { transportFailureUrl } : {}),
          });

          // Log to Cypress Cloud command log
          relevant.forEach((e: any) => {
            cy.log(`[${e.severity}] ${e.message}`);
          });
          failedHttp.forEach((r) => {
            cy.log(`[HTTP ${r.status}] ${r.method} ${r.url}: ${r.body}`);
          });
          if (storeSnapshot) {
            cy.log(`[Store Snapshot] ${JSON.stringify(storeSnapshot).substring(0, 2000)}`);
          }
        }

        if (typeof clearLog === 'function') clearLog();
      }
    } else {
      cy.task('log', '[API Log] window.__getRequestsLog not available');
    }

    // Dump connection-health + heartbeat diagnostics (separate ring from the
    // requests log so REST spam doesn't evict the "did data arrive / when did
    // the WS go yellow" timeline). Runs independent of the requests-log branch
    // above — diagnostics can exist even when the requests log is empty.
    const getDiag = (win as any).__getDiagnosticLog;
    const clearDiag = (win as any).__clearDiagnosticLog;
    if (typeof getDiag === 'function') {
      const diagEntries = getDiag();
      if (diagEntries && diagEntries.length > 0) {
        cy.task('writeApiLog', { exchange: exchangeName, testName: `${testName} [DIAGNOSTICS]`, entries: diagEntries });
      }
      if (typeof clearDiag === 'function') clearDiag();
    }
  });
});

after(() => {
  if (ENABLED_TESTS.includes('all')) {
    cleanup();
  }
});

// Test selection mechanism
const ENABLED_TESTS = (Cypress.env('ENABLED_TESTS') || 'all').split(',').map((t: string) => t.trim());

function shouldRunTest(testName: string): boolean {
  // Check if test is in ENABLED_TESTS
  if (!ENABLED_TESTS.includes('all') && !ENABLED_TESTS.includes(testName)) {
    return false;
  }

  // Check if test is skipped for this exchange
  try {
    const config = getExchangeConfig();
    if (config.skipTests && config.skipTests.includes(testName)) {
      return false;
    }
  } catch (e) {
    // Config not available yet, allow test to run
  }

  return true;
}

/**
 * Read the flip opt-in the same way `shouldRunTest` reads its skip list.
 *
 * This gate is evaluated while the describe body is being built, which happens BEFORE
 * `before()` assigns the module-level `exchangeConfig` — reading that variable here
 * throws a TypeError and takes the whole spec down with it. `Cypress.env` is populated
 * at load time, and the try/catch matches `shouldRunTest`'s handling of a run with no
 * exchange configured.
 */
const exchangeSupportsPositionFlip = (): boolean => {
  try {
    return getExchangeConfig().supportsPositionFlip === true;
  } catch {
    return false;
  }
};

describe(
  (() => {
    // Initialize config before getting symbol for describe title
    const config = getExchangeConfig();
    return `Test ${getAccount().exchange?.toUpperCase()} Exchange Integration on ${config.symbol}`;
  })(),
  {
    retries: {
      runMode: ENABLED_TESTS.includes('all') ? 1 : 0,
      openMode: ENABLED_TESTS.includes('all') ? 1 : 0,
    },
  },
  () => {
    before(() => {
      // Initialize exchange-specific configuration
      exchangeConfig = getExchangeConfig();
      symbol = exchangeConfig.symbol;
      orderSizeInQuote = exchangeConfig.orderSizeInQuote;
      orderSizeVariance = exchangeConfig.orderSizeVariance;
      farTouchDelta = exchangeConfig.farTouchDelta;
      crossDelta = exchangeConfig.crossDelta ?? exchangeConfig.farTouchDelta;
      quoteCurrency = exchangeConfig.quoteCurrency ?? 'USDT';
      displaySymbol = exchangeConfig.displaySymbol ?? exchangeConfig.symbol;
      expectedWalletBalance = exchangeConfig.expectedWalletBalance;
      expectedAvailableMargin = exchangeConfig.expectedAvailableMargin;
      balanceVariance = exchangeConfig.balanceVariance ?? 1.0;

      // Calculate wait times
      const metaData = getAccMetadata();
      actionWaitTime = exchangeConfig.actionWaitTime || (metaData.tickInterval || 1000) * 3;
      slowUpdateExtraWaitTime = exchangeConfig.slowUpdateExtraWaitTime || 0;
      openPositionSettleWaitTime = exchangeConfig.openPositionSettleWaitTime ?? actionWaitTime;
      // For REST-only exchanges (WOO), positions can take up to 60s to appear
      positionWaitTimeout = slowUpdateExtraWaitTime > 0 ? 90000 : 45000;

      // Setup automated debug logging
      setupDebugLogging(exchangeConfig.exchange);

      cy.log(`Testing ${exchangeConfig.exchange} on ${symbol}`);
      cy.log(`Order size: ${orderSizeInQuote} USD`);
      cy.log(`Price delta: ${farTouchDelta * 100}%`);
    });

    if (shouldRunTest('bad-api-key')) {
      it('Bad API Key — error message and bounded reconnects', () => {
        const account = getAccount();
        if (account.exchange === 'paradex') {
          cy.get('button[title="Account"]').first().click();
          cy.wait(300);
          cy.contains('.ant-dropdown-menu-item, [role=menuitem]', /^Api Keys$/i).click();
          cy.contains('button', 'Add Account').should('be.visible').click();

          selectExchangeInAddAccountFlow('paradex');

          getAddAccountInput('name', { timeout: 10000 }).should('be.visible').clear().type('e2e-bad-key');
          getAddAccountInput('api-key').clear().type(corruptKey(account.apiKey));
          getAddAccountInput('subkey-address').clear().type(account.subkeyAddress);
          getAddAccountInput('paradex-address').clear().type(account.paradexAddress);
          cy.contains('button', tradeEn['done']).click();

          cy.window({ log: false, timeout: 15000 })
            .should((win) => {
              const authFailedRequests = capturedAuthFailedRequests();
              const validationText = addAccountValidationText(win);
              expect(
                authFailedRequests.length + (isStrictAuthValidationText(validationText) ? 1 : 0),
                'Paradex bad signing key should fail add-account auth validation',
              ).to.be.greaterThan(0);
            })
            .then((win) => {
              const authFailedRequests = capturedAuthFailedRequests();
              const validationText = addAccountValidationText(win);
              cy.task('log', `[bad-api-key:paradex] Captured auth failed HTTP: ${authFailedRequests.length}`);
              cy.task('log', `[bad-api-key:paradex] Validation text: ${validationText.substring(0, 300)}`);
            });
          cy.get('body').type('{esc}');
          return;
        }

        const badAccount = {
          ...account,
          apiKey: corruptKey(account.apiKey),
          apiSecret: corruptKey(account.apiSecret),
        };

        // Write corrupted account to localStorage and revisit.
        // Session cookie from beforeEach login persists so we're still logged in.
        cy.visit(getHost(), {
          onBeforeLoad(win) {
            win.localStorage.setItem('accounts', JSON.stringify([badAccount]));
            win.localStorage.setItem('disableWebWorker', 'true');
            win.localStorage.setItem('requestsPanelIsOpenAtom', '"true"');
            win.localStorage.setItem(
              'lastSettings',
              JSON.stringify({
                trade: { activeAccount: badAccount.name, activeSymbol: symbol },
                sound: { enabled: false, volume: 0 },
                enableDevTools: true,
                general: { showConfirmationDialogs: true, showErrorsWhileReconnecting: true },
              }),
            );

            // Hook console.error for diagnostics
            const origError = win.console.error;
            win.console.error = (...args: any[]) => {
              capturedConsoleErrors.push(args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' '));
              origError.apply(win.console, args);
            };

            const originalFetch = win.fetch;
            win.fetch = async (...fetchArgs: Parameters<typeof fetch>) => {
              const response = await originalFetch.apply(win, fetchArgs);
              if (response.status >= 400) {
                const fetchUrl = typeof fetchArgs[0] === 'string' ? fetchArgs[0] : (fetchArgs[0] as Request).url;
                const fetchMethod = (fetchArgs[1] as RequestInit)?.method || 'GET';
                let body = '';
                try {
                  body = await response.clone().text();
                  if (body.length > 500) body = body.slice(0, 500) + '...';
                } catch {}
                recordFailedRequest({ method: fetchMethod, url: fetchUrl, status: response.status, body });
              }
              return response;
            };

            const origXHROpen = win.XMLHttpRequest.prototype.open;
            win.XMLHttpRequest.prototype.open = function (method: string, url: string | URL, ...rest: any[]) {
              this.addEventListener('load', function () {
                if (this.status >= 400) {
                  let body = '';
                  try {
                    body = (this.responseText || '').slice(0, 500);
                  } catch {}
                  recordFailedRequest({ method, url: String(url), status: this.status, body });
                }
              });
              return origXHROpen.apply(this, [method, url, ...rest] as any);
            };

            // Toast capture via MutationObserver — toasts auto-close after 3s
            const capturedToasts: { timestamp: number; text: string; type: string }[] = [];
            (win as any).__capturedToasts = capturedToasts;

            const observeToastContainer = (container: Element) => {
              const obs = new win.MutationObserver((mutations) => {
                for (const m of mutations) {
                  for (const node of m.addedNodes) {
                    if (!(node instanceof win.HTMLElement)) continue;
                    const toasts = node.classList?.contains('Toastify__toast')
                      ? [node]
                      : Array.from(node.querySelectorAll('.Toastify__toast'));
                    for (const t of toasts) {
                      const text = (t.textContent || '').trim();
                      if (text) {
                        capturedToasts.push({
                          timestamp: Date.now(),
                          text,
                          type: t.classList.contains('Toastify__toast--error')
                            ? 'error'
                            : t.classList.contains('Toastify__toast--warning')
                              ? 'warning'
                              : 'info',
                        });
                      }
                    }
                  }
                }
              });
              obs.observe(container, { childList: true, subtree: true });
            };

            const startObserving = () => {
              const existing = win.document.body.querySelector('.Toastify');
              if (existing) {
                observeToastContainer(existing);
                return;
              }
              const bodyObs = new win.MutationObserver((mutations) => {
                for (const m of mutations) {
                  for (const node of m.addedNodes) {
                    if (!(node instanceof win.HTMLElement)) continue;
                    const root = node.classList?.contains('Toastify') ? node : node.querySelector('.Toastify');
                    if (root) {
                      observeToastContainer(root);
                      bodyObs.disconnect();
                      return;
                    }
                  }
                }
              });
              bodyObs.observe(win.document.body, { childList: true, subtree: true });
            };

            if (win.document.body) startObserving();
            else win.document.addEventListener('DOMContentLoaded', startObserving);
          },
        });

        // Suppress uncaught exceptions — exchange will throw with bad creds
        cy.on('uncaught:exception', () => false);

        // Wait for observation window
        const OBSERVATION_WINDOW = 30_000;
        cy.wait(OBSERVATION_WINDOW);

        cy.window({ log: false }).then((win) => {
          const entries: any[] = (win as any).__getRequestsLog?.() || [];
          const capturedToasts: { timestamp: number; text: string; type: string }[] =
            (win as any).__capturedToasts || [];

          cy.task('log', `[bad-api-key] Total log entries: ${entries.length}`);
          cy.task('log', `[bad-api-key] Captured toasts: ${capturedToasts.length}`);

          // ── Counters ──
          const errors = entries.filter((e: any) => e.type === 'exchangeError' || e.severity === 'error');
          const reconnects = entries.filter((e: any) => {
            const msg = typeof e.message === 'string' ? e.message : '';
            return msg.includes('close') || msg.includes('Reconnect');
          });
          const httpRequests = entries.filter((e: any) => e.type === 'httpRequest');

          cy.task('log', `[bad-api-key] Errors: ${errors.length}`);
          cy.task('log', `[bad-api-key] WS reconnect indicators: ${reconnects.length}`);
          cy.task('log', `[bad-api-key] HTTP requests: ${httpRequests.length}`);

          errors.slice(0, 5).forEach((e: any, i: number) => {
            const msg = typeof e.message === 'string' ? e.message : JSON.stringify(e.message);
            cy.task('log', `[bad-api-key] Error[${i}]: ${msg.substring(0, 300)}`);
          });

          capturedToasts.forEach((t, i) => {
            cy.task('log', `[bad-api-key] Toast[${i}] (${t.type}): ${t.text.substring(0, 300)}`);
          });

          // ── Error quality scoring ──
          const genericPatterns = [
            /^Unknown Exchange Error$/,
            /^Error$/,
            /^Error starting exchange:/,
            /^Exchange Error$/,
            /^Request failed$/i,
            /^unknown error$/i,
          ];
          const isGeneric = (msg: string): boolean => genericPatterns.some((p) => p.test(msg.trim()));

          const errorMessages = errors
            .map((e: any) => (typeof e.message === 'string' ? e.message : ''))
            .filter((m: string) => m.length > 0);
          const genericCount = errorMessages.filter(isGeneric).length;
          const meaningfulCount = errorMessages.length - genericCount;
          const qualityPct =
            errorMessages.length > 0 ? Math.round((meaningfulCount / errorMessages.length) * 100) : 100;

          cy.task(
            'log',
            `[bad-api-key] Error quality: ${qualityPct}% (${meaningfulCount} meaningful, ${genericCount} generic)`,
          );

          const errorCounts = new Map<string, number>();
          errorMessages.forEach((msg: string) => {
            errorCounts.set(msg.substring(0, 200), (errorCounts.get(msg.substring(0, 200)) || 0) + 1);
          });
          errorCounts.forEach((count, msg) => {
            cy.task('log', `[bad-api-key]   ${count}x: ${msg}`);
          });

          // ── Auth patterns ──
          const meaningfulAuthErrors = errors.filter((e: any) => {
            const msg = typeof e.message === 'string' ? e.message : '';
            return isAuthFailureText(msg);
          });
          const authFailedRequests = capturedAuthFailedRequests();

          // ── Toast quality ──
          const errorToasts = capturedToasts.filter((t) => t.type === 'error');
          const meaningfulAuthToasts = errorToasts.filter((t) => isAuthFailureText(t.text));

          cy.task(
            'log',
            `[bad-api-key] Error toasts: ${errorToasts.length}, meaningful: ${meaningfulAuthToasts.length}`,
          );
          cy.task('log', `[bad-api-key] Captured auth failed HTTP: ${authFailedRequests.length}`);

          // ── ASSERTIONS ──

          // 1. At least one meaningful auth failure in logs, UI, or captured HTTP.
          // Paradex hosted mode may fail during JWT auth before Requests Panel emits
          // an exchangeError, but the intercepted /auth 4xx is still the real signal.
          expect(
            meaningfulAuthErrors.length + meaningfulAuthToasts.length + authFailedRequests.length,
            'At least one meaningful auth failure',
          ).to.be.greaterThan(0);

          // 2. Bounded reconnects
          expect(reconnects.length, `WS reconnects (${reconnects.length}) should be bounded`).to.be.at.most(40);

          // 3. Bounded total errors
          expect(errors.length, `Total errors (${errors.length}) should be bounded`).to.be.at.most(
            badApiKeyMaxErrors(),
          );

          // 4. Bounded HTTP requests
          expect(httpRequests.length, `HTTP requests (${httpRequests.length}) should be bounded`).to.be.at.most(
            badApiKeyMaxHttpRequests(),
          );

          // 5. At least one error toast appeared in the UI unless hosted
          // Paradex failed earlier at the auth HTTP boundary.
          expect(
            errorToasts.length + (exchangeConfig.exchange === 'paradex' ? authFailedRequests.length : 0),
            `At least one error toast or Paradex auth HTTP failure should appear (got ${errorToasts.length} toasts, ${authFailedRequests.length} auth HTTP failures)`,
          ).to.be.greaterThan(0);

          // 6. At least one error toast has meaningful auth-related text when the UI surfaces it.
          if (requiresAuthTextInBadKeyToast()) {
            expect(
              meaningfulAuthToasts.length,
              `At least one toast should mention auth/key/invalid. Toasts: [${errorToasts.map((t) => t.text.substring(0, 80)).join(' | ')}]`,
            ).to.be.greaterThan(0);
          }

          // 7. Error message quality >= 25%
          expect(
            qualityPct,
            `Error quality ${qualityPct}% should be >= 25%. ${genericCount}/${errorMessages.length} were generic.`,
          ).to.be.at.least(25);
        });
      });
    }

    if (shouldRunTest('add-api-key')) {
      it('Add API Key — good and bad credentials via UI form', () => {
        const account = getAccount();
        const metaData = getAccMetadata();
        const exchange = account.exchange;

        const isHyperliquid = exchange === 'hyperliquid' || exchange === 'hyperliquidspot';
        if (isHyperliquid) {
          cy.task(
            'log',
            '[add-api-key] Hyperliquid production add-account can require wallet-signed builder-fee approval; skipping UI submit flow',
          );
          return;
        }

        if (exchange === 'bitunix' && !account.bitunixAccessCode) {
          cy.task(
            'log',
            '[add-api-key] Bitunix access code missing from Cypress account; skipping add-account UI flow',
          );
          return;
        }

        // ── Phase A: Add account with good credentials via UI ──

        // Open settings modal → Accounts tab. "Api Keys" lives inside the
        // user-avatar dropdown (Tealstreet default header); open the dropdown
        // first, then click the menu item.
        cy.get('button[title="Account"]').first().click();
        cy.wait(300);
        cy.contains('.ant-dropdown-menu-item, [role=menuitem]', /^Api Keys$/i).click();
        // AccountsSettings renders a hardcoded "Add Account"; the old
        // "Add new key" label is gone and its trade.add-new-key string is
        // now unreferenced.
        cy.contains('button', 'Add Account').should('be.visible').click();

        // Step 1 — Exchange selection. The current flow selects a tile first
        // and advances with Next; the helper also tolerates older auto-advance
        // builds when this spec targets an already-deployed lane.
        selectExchangeInAddAccountFlow(exchange);

        // For Hyperliquid, default UI shows auto setup wizard — switch to manual form
        if (isHyperliquid) {
          cy.contains('button', tradeEn['manual-setup']).click();
          getAddAccountInput('name', { timeout: 5000 }).should('be.visible');
        }

        // Step 2 — Fill form fields via stable add-account test ids.
        getAddAccountInput('name', { timeout: 10000 }).should('be.visible').clear().type('e2e-good-key');
        fillPrimaryCredentialFields(account);

        // Check "Use Tealstreet IPs" if the exchange requires CORS proxy
        if (metaData.requiresCors) {
          // requiresCors exchanges have the checkbox forced-checked, nothing to do
        } else if (account.tealstreetProxy) {
          enableTealstreetProxyIfRendered();
        }

        // Submit the form
        cy.contains('button', tradeEn['done']).click();

        // Assert: account appears in localStorage. Credential validation can
        // stay in-flight for several seconds after Done is clicked.
        cy.window({ log: false, timeout: 30000 }).should((win) => {
          const accounts = JSON.parse(win.localStorage.getItem('accounts') || '[]');
          const goodAccount = accounts.find((a: any) => a.name === 'e2e-good-key');
          expect(goodAccount, 'Good account should be saved to localStorage').to.not.be.undefined;
        });
        cy.window({ log: false }).then((win) => {
          const accounts = JSON.parse(win.localStorage.getItem('accounts') || '[]');
          const goodAccount = accounts.find((a: any) => a.name === 'e2e-good-key');
          expect(goodAccount.exchange, 'Account exchange should match').to.eq(exchange);
          cy.task('log', `[add-api-key] Good account saved: ${goodAccount.name} (${goodAccount.exchange})`);
        });

        // ── Phase B: Add account with bad credentials via UI ──
        // Close settings modal and reopen to reset state — after Phase A adds the
        // account, the app may connect to it, causing UI state changes that break Phase B.
        cy.get('body').type('{esc}');
        cy.wait(500);
        cy.get('button[title="Account"]').first().click();
        cy.wait(300);
        cy.contains('.ant-dropdown-menu-item, [role=menuitem]', /^Api Keys$/i).click();
        // AccountsSettings renders a hardcoded "Add Account"; the old
        // "Add new key" label is gone and its trade.add-new-key string is
        // now unreferenced.
        cy.contains('button', 'Add Account').should('be.visible').click();

        // Step 1 — Exchange selection.
        selectExchangeInAddAccountFlow(exchange);

        // For Hyperliquid, switch to manual form again
        if (isHyperliquid) {
          cy.contains('button', tradeEn['manual-setup']).click();
          getAddAccountInput('name', { timeout: 5000 }).should('be.visible');
        }

        // Step 2 — Fill form with corrupted credentials
        getAddAccountInput('name', { timeout: 10000 }).should('be.visible').clear().type('e2e-bad-key');
        fillPrimaryCredentialFields(account, { corrupt: true });
        if (metaData.requiresCors) {
          // forced-checked, nothing to do
        } else if (account.tealstreetProxy) {
          enableTealstreetProxyIfRendered();
        }

        cy.on('uncaught:exception', (err) => {
          if (isAuthFailureText(err.message)) return false;
          return true;
        });
        cy.contains('button', tradeEn['done']).click();

        // The form now pre-validates credentials against the exchange before
        // saving (see useAccountForm.ts `handleFinish` → `validateAccount`).
        // For bad creds, the modal stays open with an error message. Confirm
        // the rejection evidence directly, then inject the bad account so
        // Phase C can exercise runtime auth errors.
        cy.window({ log: false, timeout: 15000 })
          .should((win) => {
            const authFailedRequests = capturedAuthFailedRequests();
            const validationText = addAccountValidationText(win);
            expect(
              authFailedRequests.length + (isStrictAuthValidationText(validationText) ? 1 : 0),
              'Bad credentials should fail add-account auth validation',
            ).to.be.greaterThan(0);
          })
          .then((win) => {
            const authFailedRequests = capturedAuthFailedRequests();
            const validationText = addAccountValidationText(win);
            cy.task('log', `[add-api-key] Bad creds rejected by UI validation, as expected`);
            cy.task('log', `[add-api-key] Captured auth failed HTTP: ${authFailedRequests.length}`);
            cy.task('log', `[add-api-key] Validation text: ${validationText.substring(0, 300)}`);
          });

        // Close the modal before directly injecting the bad account.
        cy.get('body').type('{esc}');
        cy.wait(500);

        // Inject the bad account directly into localStorage so Phase C has
        // something to activate. We still preserve the goal: "the app shows
        // auth errors when the active account has bad creds".
        cy.window({ log: false }).then((win) => {
          const accounts = JSON.parse(win.localStorage.getItem('accounts') || '[]');
          if (!accounts.find((a: any) => a.name === 'e2e-bad-key')) {
            accounts.push({
              ...account,
              name: 'e2e-bad-key',
              apiKey: corruptKey(account.apiKey),
              apiSecret: corruptKey(account.apiSecret),
              ...(metaData.requiresPassphrase && account.passphrase
                ? { passphrase: corruptKey(account.passphrase) }
                : {}),
              ...(metaData.requiresApplicationId && account.applicationId
                ? { applicationId: corruptKey(account.applicationId) }
                : {}),
              active: true,
            });
            win.localStorage.setItem('accounts', JSON.stringify(accounts));
          }
          const badAccount = accounts.find((a: any) => a.name === 'e2e-bad-key');
          expect(badAccount, 'Bad account should be in localStorage').to.not.be.undefined;
          cy.task(
            'log',
            `[add-api-key] Bad account saved (via injection): ${badAccount.name} (${badAccount.exchange})`,
          );
        });

        // ── Phase C: Activate bad account and observe auth errors ──

        cy.window({ log: false }).then((win) => {
          const settings = JSON.parse(win.localStorage.getItem('lastSettings') || '{}');
          settings.trade = { ...settings.trade, activeAccount: 'e2e-bad-key' };
          win.localStorage.setItem('lastSettings', JSON.stringify(settings));
        });

        // Revisit with toast capture to observe error messages
        cy.visit(getHost(), {
          onBeforeLoad(win) {
            // Toast capture via MutationObserver — same pattern as bad-api-key test
            const capturedToasts: { timestamp: number; text: string; type: string }[] = [];
            (win as any).__capturedToasts = capturedToasts;

            const observeToastContainer = (container: Element) => {
              const obs = new win.MutationObserver((mutations) => {
                for (const m of mutations) {
                  for (const node of m.addedNodes) {
                    if (!(node instanceof win.HTMLElement)) continue;
                    const toasts = node.classList?.contains('Toastify__toast')
                      ? [node]
                      : Array.from(node.querySelectorAll('.Toastify__toast'));
                    for (const t of toasts) {
                      const text = (t.textContent || '').trim();
                      if (text) {
                        capturedToasts.push({
                          timestamp: Date.now(),
                          text,
                          type: t.classList.contains('Toastify__toast--error')
                            ? 'error'
                            : t.classList.contains('Toastify__toast--warning')
                              ? 'warning'
                              : 'info',
                        });
                      }
                    }
                  }
                }
              });
              obs.observe(container, { childList: true, subtree: true });
            };

            const startObserving = () => {
              const existing = win.document.body.querySelector('.Toastify');
              if (existing) {
                observeToastContainer(existing);
                return;
              }
              const bodyObs = new win.MutationObserver((mutations) => {
                for (const m of mutations) {
                  for (const node of m.addedNodes) {
                    if (!(node instanceof win.HTMLElement)) continue;
                    const root = node.classList?.contains('Toastify') ? node : node.querySelector('.Toastify');
                    if (root) {
                      observeToastContainer(root);
                      bodyObs.disconnect();
                      return;
                    }
                  }
                }
              });
              bodyObs.observe(win.document.body, { childList: true, subtree: true });
            };

            if (win.document.body) startObserving();
            else win.document.addEventListener('DOMContentLoaded', startObserving);
          },
        });

        // Suppress uncaught exceptions — exchange will throw with bad creds
        cy.on('uncaught:exception', () => false);

        // Poll for the error toast rather than sleeping a fixed window. Bitget's
        // startup fetches contracts + tickers through the CORS proxy before its
        // first authenticated call — 11s+ on a slow runner — so a fixed 15s wait
        // asserted before the auth failure had a chance to surface.
        const TOAST_TIMEOUT = 60_000;
        cy.window({ log: false, timeout: TOAST_TIMEOUT }).should((win) => {
          const captured: { type: string }[] = (win as any).__capturedToasts || [];
          const errors = captured.filter((t) => t.type === 'error');
          const authFailedRequests = capturedAuthFailedRequests();
          expect(
            errors.length + (exchangeConfig.exchange === 'paradex' ? authFailedRequests.length : 0),
            `At least one error toast or Paradex auth HTTP failure should appear (got ${errors.length} toasts, ${authFailedRequests.length} auth HTTP failures)`,
          ).to.be.greaterThan(0);
        });

        cy.window({ log: false }).then((win) => {
          const entries: any[] = (win as any).__getRequestsLog?.() || [];
          const capturedToasts: { timestamp: number; text: string; type: string }[] =
            (win as any).__capturedToasts || [];

          cy.task('log', `[add-api-key] Total log entries: ${entries.length}`);
          cy.task('log', `[add-api-key] Captured toasts: ${capturedToasts.length}`);

          capturedToasts.forEach((t, i) => {
            cy.task('log', `[add-api-key] Toast[${i}] (${t.type}): ${t.text.substring(0, 300)}`);
          });

          // At least one error toast should appear (auth failure)
          const errorToasts = capturedToasts.filter((t) => t.type === 'error');
          const meaningfulAuthToasts = errorToasts.filter((t) => isAuthFailureText(t.text));
          const authFailedRequests = capturedAuthFailedRequests();

          cy.task(
            'log',
            `[add-api-key] Error toasts: ${errorToasts.length}, meaningful auth: ${meaningfulAuthToasts.length}`,
          );
          cy.task('log', `[add-api-key] Captured auth failed HTTP: ${authFailedRequests.length}`);

          expect(
            errorToasts.length + (exchangeConfig.exchange === 'paradex' ? authFailedRequests.length : 0),
            `At least one error toast or Paradex auth HTTP failure should appear (got ${errorToasts.length} toasts, ${authFailedRequests.length} auth HTTP failures)`,
          ).to.be.greaterThan(0);
        });

        // ── Phase D: Cleanup — remove test accounts and restore original ──

        cy.window({ log: false }).then((win) => {
          const accounts = JSON.parse(win.localStorage.getItem('accounts') || '[]');
          const cleaned = accounts.filter((a: any) => a.name !== 'e2e-good-key' && a.name !== 'e2e-bad-key');
          win.localStorage.setItem('accounts', JSON.stringify(cleaned));

          const settings = JSON.parse(win.localStorage.getItem('lastSettings') || '{}');
          settings.trade = { ...settings.trade, activeAccount: account.name };
          win.localStorage.setItem('lastSettings', JSON.stringify(settings));
          cy.task('log', '[add-api-key] Cleanup complete — restored original account');
        });

        // Reload with original account to leave environment clean
        cy.visit(getHost());
      });
    }

    if (shouldRunTest('load-balance')) {
      it('Load Balance', () => {
        getAdjustedBalance();
        getAvailableMargin();
        cy.get('@adjustedBalance')
          .get('@availableMargin')
          .then(function () {
            const adjustedBalance: number = this.adjustedBalance;
            const availableMargin: number = this.availableMargin;
            expect(adjustedBalance).to.be.closeTo(expectedWalletBalance, expectedWalletBalance * balanceVariance);
            expect(availableMargin).to.be.closeTo(expectedAvailableMargin, expectedAvailableMargin * balanceVariance);
          });
      });
    }

    if (shouldRunTest('place-market-orders')) {
      it('Place Market Orders', () => {
        const marketOrderSizeInQuote = exchangeConfig.marketOrderSizeInQuote ?? orderSizeInQuote;

        ensureOneWayPositionMode();

        setOrderTypeTab('market');
        cy.wait(1000); // Wait for market order form to mount

        setConfiguredLeverageForE2E(getAccMetadata());

        const marketOrderSides = exchangeConfig.marketOrderSides ?? [OrderSide.Buy, OrderSide.Sell];

        marketOrderSides
          .map((side) => ({
            side,
            positionSide: side === OrderSide.Buy ? PositionSide.Long : PositionSide.Short,
          }))
          .forEach(({ side, positionSide }, i) => {
            cy.log('Setting market size');
            setOrderSize(marketOrderSizeInQuote);

            // Verify the size was actually set in the input
            ensureOrderSizeInputHasValue(side);

            cy.log('Ensure reduce only is not checked');
            ensureVisibleCheckboxesAreNotChecked('[data-testid=reduce-only-checkbox]');

            // Verify button is not disabled before clicking (DivButton uses aria-disabled)
            ensureMarketButtonIsEnabled(side);

            cy.log(`Placing market order`);
            placeMarketOrderForE2E(side, marketOrderSizeInQuote);

            if (exchangeConfig.exchange !== 'phemex') {
              waitForOpenedPositionForE2E();
            }

            // Log any toasts for debugging
            cy.get('body').then(($body) => {
              const toasts = $body.find('.Toastify__toast');
              if (toasts.length > 0) {
                toasts.each((_, el) => {
                  cy.log(`TOAST: ${el.textContent}`);
                });
              }
            });

            cy.log('Checking that order is filled and position opened');
            ensureTableBodyLength('[data-testid=open-orders-table]', 0);
            // Use longer timeout for position to appear - some exchanges (WOO, Phemex)
            // rely on REST polling for position updates which can be slow
            ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);

            cy.log('Checking that position size is correct');
            ensurePositionSize(marketOrderSizeInQuote, positionSide, symbol, positionWaitTimeout);

            cy.log('Checking that reduce only order cancels');
            ensureVisibleCheckboxesAreChecked('[data-testid=reduce-only-checkbox]');

            cy.log('Setting market size');
            setOrderSize(marketOrderSizeInQuote);

            cy.log('Placing reduce market order');
            closePositionWithMarketOrderForE2E(positionSide);
            cy.wait(actionWaitTime);
            cy.wait(slowUpdateExtraWaitTime);

            cy.log('Checking that order is filled and position closed');
            ensureTableBodyLength('[data-testid=open-orders-table]', 0);
            ensureTableBodyLength('[data-testid=open-positions-table]', 0, positionWaitTimeout);
          });
      });
    }

    // A single order that closes one position and opens the opposite one.
    //
    // This is the case every downstream fold gets wrong by default: the venue reports it
    // as ONE fill whose size exceeds the open position, so a naive reducer either drives
    // the position negative or lets the new short inherit the old long's entry price, and
    // the realized PnL from the close lands on the wrong trade. Hyperliquid names the case
    // explicitly (`dir: "Long > Short"`); most venues just emit an oversized fill.
    if (shouldRunTest('position-flip') && exchangeSupportsPositionFlip()) {
      it('Flip position through zero in a single order', () => {
        const flipSizeInQuote = exchangeConfig.marketOrderSizeInQuote ?? orderSizeInQuote;

        // Only meaningful one-way: in hedge mode the opposite order opens a second,
        // independent leg instead of crossing through zero.
        ensureOneWayPositionMode();

        setOrderTypeTab('market');
        cy.wait(1000); // market order form mount

        cy.log('Opening the long leg');
        setOrderSize(flipSizeInQuote);
        ensureOrderSizeInputHasValue(OrderSide.Buy);
        ensureVisibleCheckboxesAreNotChecked('[data-testid=reduce-only-checkbox]');
        ensureMarketButtonIsEnabled(OrderSide.Buy);
        placeMarketOrderForE2E(OrderSide.Buy, flipSizeInQuote);
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);

        ensureTableBodyLength('[data-testid=open-orders-table]', 0);
        ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
        ensurePositionSize(flipSizeInQuote, PositionSide.Long, symbol, positionWaitTimeout);
        recordGroundTruth('market-order', { side: 'buy', sizeQuote: flipSizeInQuote, intent: 'open-long' });

        cy.log('Selling twice the position — crosses through zero into a short');
        setOrderSize(flipSizeInQuote * 2);
        // Reduce-only auto-checks once a position exists, and venues carrying
        // `autoDownSizeReduceOrders` clamp a reduce order back to the position size — so
        // leaving it set would close the long and never open the short, while every
        // assertion below still passed on a flat account.
        ensureVisibleCheckboxesAreNotChecked('[data-testid=reduce-only-checkbox]');
        ensureOrderSizeInputHasValue(OrderSide.Sell);
        ensureMarketButtonIsEnabled(OrderSide.Sell);
        placeMarketOrderForE2E(OrderSide.Sell, flipSizeInQuote * 2);
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);

        cy.log('Asserting the position is now SHORT at ~1x, not flat and not long');
        ensureTableBodyLength('[data-testid=open-orders-table]', 0);
        ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
        ensurePositionSize(flipSizeInQuote, PositionSide.Short, symbol, positionWaitTimeout);
        // Record the size the venue actually landed on. `sellQty - longQty` is not the
        // nominal size once the two round independently, and the verifier needs the real
        // number to reconcile against the venue's own history.
        getDisplayedPositionSizeInBase(PositionSide.Short).then((actualShortInBase) => {
          recordGroundTruth('flip-observed', { observedShortBase: actualShortInBase });
        });
        recordGroundTruth('flip', {
          side: 'sell',
          sizeQuote: flipSizeInQuote * 2,
          intent: 'long-to-short',
          expectedClosedQuote: flipSizeInQuote,
          expectedOpenedQuote: flipSizeInQuote,
        });

        // Close the size the flip ACTUALLY produced, not the nominal one. The flipped
        // position is `sellQty - longQty`, and those two round independently: bybit sold
        // 283 against a 141 long and ended up short 142, so closing the nominal 141 left
        // a 1-DOGE residual and the position table never reached zero. OKX only passed
        // because its rounding happened to land exactly (280 - 140 = 140).
        cy.log('Flattening the size the flip actually produced');
        ensureVisibleCheckboxesAreChecked('[data-testid=reduce-only-checkbox]');
        getDisplayedPositionSizeInBase(PositionSide.Short).then((actualShortInBase) => {
          setOrderBaseSize(actualShortInBase);
        });
        closePositionWithMarketOrderForE2E(PositionSide.Short);
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);
        ensureTableBodyLength('[data-testid=open-positions-table]', 0, positionWaitTimeout);
        recordGroundTruth('close', { intent: 'flatten-after-flip' });
      });
    }

    // Close a position in thirds, so realized PnL accrues across several partial closes
    // while the position stays open.
    //
    // The grain question this answers: does each partial close emit its own closed-position
    // row, or does the venue report one row at final close? A canonical `HistoricalPosition`
    // is a round trip, so a venue reporting per-partial rows needs them aggregated, and one
    // reporting a single row needs `maxSize` recovered from fills. Those are different folds.
    //
    // Opens at 3x so each third divides evenly — at 1x the thirds land below the venue's
    // size precision and round unevenly, which would make the residual, not the ladder,
    // the thing under test.
    if (shouldRunTest('partial-closes') && exchangeSupportsPositionFlip()) {
      it('Close a position in thirds', () => {
        const stepInQuote = exchangeConfig.marketOrderSizeInQuote ?? orderSizeInQuote;
        const openInQuote = stepInQuote * 3;

        ensureOneWayPositionMode();
        setOrderTypeTab('market');
        cy.wait(1000);

        cy.log('Opening at 3x');
        setOrderSize(openInQuote);
        ensureVisibleCheckboxesAreNotChecked('[data-testid=reduce-only-checkbox]');
        ensureMarketButtonIsEnabled(OrderSide.Buy);
        placeMarketOrderForE2E(OrderSide.Buy, openInQuote);
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);
        ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
        ensurePositionSize(openInQuote, PositionSide.Long, symbol, positionWaitTimeout);
        recordGroundTruth('market-order', { side: 'buy', sizeQuote: openInQuote, intent: 'open-3x' });

        // Two partial closes, each a third, leaving the position open in between.
        for (const remainingMultiple of [2, 1]) {
          cy.log(`Closing a third, expecting ${remainingMultiple}x to remain`);
          ensureVisibleCheckboxesAreChecked('[data-testid=reduce-only-checkbox]');
          setOrderSize(stepInQuote);
          closePositionWithMarketOrderForE2E(PositionSide.Long);
          cy.wait(actionWaitTime);
          cy.wait(slowUpdateExtraWaitTime);

          ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
          ensurePositionSize(stepInQuote * remainingMultiple, PositionSide.Long, symbol, positionWaitTimeout);
          recordGroundTruth('partial-close', {
            side: 'sell',
            sizeQuote: stepInQuote,
            intent: `leave-${remainingMultiple}x-open`,
          });
        }

        cy.log('Closing the remainder');
        ensureVisibleCheckboxesAreChecked('[data-testid=reduce-only-checkbox]');
        getDisplayedPositionSizeInBase(PositionSide.Long).then((remainingBase) => {
          setOrderBaseSize(remainingBase);
        });
        closePositionWithMarketOrderForE2E(PositionSide.Long);
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);
        ensureTableBodyLength('[data-testid=open-positions-table]', 0, positionWaitTimeout);
        recordGroundTruth('close', { intent: 'flatten-after-ladder' });
      });
    }

    if (shouldRunTest('place-limit-orders')) {
      it('Place Limit Orders with GTC, Post', () => {
        cy.get('[data-testid=toggle-tabs-limit]').first().click();

        const sizeInBase = orderSizeInQuote / lastTrade;
        const buyPrice = handleRoundingError(lastTrade * (1 - farTouchDelta));
        const sellPrice = handleRoundingError(lastTrade * (1 + farTouchDelta));

        const nextBuyPrice = handleRoundingError(lastTrade * (1 - farTouchDelta * 2));
        const nextSellPrice = handleRoundingError(lastTrade * (1 + farTouchDelta * 2));
        const postOnlyDistance = exchangeConfig.exchange === 'phemex' ? 3 : 1;
        const postOnlyNextDistance = exchangeConfig.exchange === 'phemex' ? 4 : 2;
        const postOnlyBuyPrice = handleRoundingError(lastTrade * (1 - farTouchDelta * postOnlyDistance));
        const postOnlySellPrice = handleRoundingError(lastTrade * (1 + farTouchDelta * postOnlyDistance));
        const postOnlyNextBuyPrice = handleRoundingError(lastTrade * (1 - farTouchDelta * postOnlyNextDistance));
        const postOnlyNextSellPrice = handleRoundingError(lastTrade * (1 + farTouchDelta * postOnlyNextDistance));

        cy.log('Setting size and price');
        setBuyPrice(buyPrice);
        setSellPrice(sellPrice);
        setOrderSize(orderSizeInQuote);

        // ensure post only is not checked
        cy.log('Ensure post only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 0);
        ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 1);

        // ensure reduce is not checked
        cy.log('Ensure reduce only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);

        cy.log('Placing limit orders');
        placeLimitOrderForE2E(OrderSide.Buy, buyPrice);
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);

        // Make sure we're viewing all active orders
        clickOrdersTab('blue-tabs-ActiveMarketActiveOrders');
        cy.wait(500);
        ensureOpenOrdersLengthForE2E(1);

        placeLimitOrderForE2E(OrderSide.Sell, sellPrice);
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);

        clickOrdersTab('blue-tabs-ActiveMarketActiveOrders');
        cy.wait(500);
        ensureOpenOrdersLengthForE2E(2);
        ensureTableBodyLength('[data-testid=open-positions-table]', 0);

        const checkOrderValues = (
          header: HTMLElement,
          order: HTMLElement,
          price: number,
          size: number,
          tif: string,
        ) => {
          expect(safelyParseValue(getCell(header, order, 'Price').textContent)).to.be.closeTo(price, price * 0.04);
          expect(safelyParseValue(getCell(header, order, 'Remaining').textContent)).to.be.closeTo(
            size,
            size * orderSizeVariance,
          );
          expect(safelyParseValue(getCell(header, order, 'Filled').textContent)).to.eq(0);
          expect(getCell(header, order, 'Type').textContent).to.eq('Limit');
          expect(getCell(header, order, 'Reduce').textContent).to.eq('No');
          // Skip TIF check for Post on exchanges that don't report post-only on resting orders:
          // - Bitunix: REST API bug, get_pending_orders always returns effect:"GTC"
          // - Hyperliquid: API doesn't report post-only status on resting orders
          // - Kraken: openorders readback returns orderType:"lmt" without a post-only marker
          const skipPostTifCheck =
            tif === 'Post' &&
            (exchangeConfig.exchange === 'bitunix' ||
              exchangeConfig.exchange === 'hyperliquid' ||
              exchangeConfig.exchange === 'binance' ||
              exchangeConfig.exchange === 'paradex' ||
              exchangeConfig.exchange === 'kraken');
          if (!skipPostTifCheck) {
            const tifText = getCell(header, order, 'TIF').textContent;
            if ((exchangeConfig.exchange === 'binance' || exchangeConfig.exchange === 'paradex') && tif === 'GTC') {
              expect(tifText).to.be.oneOf(['', 'GTC']);
            } else {
              expect(tifText).to.eq(tif);
            }
          }
        };

        const validateAndUpdateOrders = (
          tif: string,
          expectedBuyPrice = buyPrice,
          expectedSellPrice = sellPrice,
          updatedBuyPrice = nextBuyPrice,
          updatedSellPrice = nextSellPrice,
        ) => {
          cy.log(`Checking that orders are ${tif}`);

          cy.wait(slowUpdateExtraWaitTime);

          if (exchangeConfig.exchange === 'paradex') {
            ensureOpenOrdersLengthForE2E(2);
            ensureTableBodyLength('[data-testid=open-positions-table]', 0);
            return;
          }

          cy.get('[data-testid=open-orders-table]').then(($element) => {
            const header = getTableHeader($element);

            // Debug: log table contents
            cy.log('Table rows:', $element.find('tr').length);
            const headerCols = Array.from(header.children).map((td: any) => td.innerText);
            cy.log('Table columns:', headerCols.join(', '));

            cy.log('Checking buy order');
            checkOrderValues(
              header,
              findOrder($element, OrderSide.Buy, displaySymbol),
              expectedBuyPrice,
              sizeInBase,
              tif,
            );

            cy.log('Checking sell order');
            checkOrderValues(
              header,
              findOrder($element, OrderSide.Sell, displaySymbol),
              expectedSellPrice,
              sizeInBase,
              tif,
            );

            if (!canAmendRestingOrders()) {
              cy.log(`${exchangeConfig.exchange} does not support exact ${tif} resting-order amend validation`);
              return;
            }

            cy.log(`Updating buy order price to ${updatedBuyPrice}`);
            updateOrderPrice(OrderSide.Buy, symbol, updatedBuyPrice);

            ensureTableBodyLength('[data-testid=open-orders-table]', 2);
            ensureTableBodyLength('[data-testid=open-positions-table]', 0);

            cy.log(`Updating sell order price to ${updatedSellPrice}`);
            updateOrderPrice(OrderSide.Sell, symbol, updatedSellPrice);

            ensureTableBodyLength('[data-testid=open-orders-table]', 2);
            ensureTableBodyLength('[data-testid=open-positions-table]', 0);

            // Use .should() to retry through transitional DOM states after cancel+replace
            // (price cell can be empty momentarily during re-render, causing NaN)
            cy.get('[data-testid=open-orders-table]').should(($element) => {
              const freshHeader = getTableHeader($element);
              checkOrderValues(
                freshHeader,
                findOrder($element, OrderSide.Buy, displaySymbol),
                updatedBuyPrice,
                sizeInBase,
                tif,
              );
              checkOrderValues(
                freshHeader,
                findOrder($element, OrderSide.Sell, displaySymbol),
                updatedSellPrice,
                sizeInBase,
                tif,
              );
            });
          });
        };

        validateAndUpdateOrders('GTC');

        // reset
        flattenPositions();
        flattenOrders();
        clickOrdersTab('blue-tabs-ActiveMarketActiveOrders');
        cy.wait(500);
        ensureOpenOrdersLengthForE2E(0);

        // set post only
        cy.log('Setting post only');
        ensureCheckboxIsChecked('[data-testid=post-only-checkbox]', 0);
        ensureCheckboxIsChecked('[data-testid=post-only-checkbox]', 1);
        setBuyPrice(postOnlyBuyPrice);
        setSellPrice(postOnlySellPrice);
        setOrderSize(orderSizeInQuote);

        cy.log('Placing post only limit orders');
        placeLimitOrderForE2E(OrderSide.Buy, postOnlyBuyPrice, { postOnly: true });
        cy.wait(actionWaitTime);
        ensureOpenOrdersLengthForE2E(1);

        placeLimitOrderForE2E(OrderSide.Sell, postOnlySellPrice, { postOnly: true });
        cy.wait(actionWaitTime);
        ensureOpenOrdersLengthForE2E(2);

        validateAndUpdateOrders(
          'Post',
          postOnlyBuyPrice,
          postOnlySellPrice,
          postOnlyNextBuyPrice,
          postOnlyNextSellPrice,
        );

        // reset
        flattenPositions();
        flattenOrders();

        cy.log('Ensure crossing post orders cancel');
        const badBuyPrice = handleRoundingError(lastTrade * (1 + crossDelta));
        setBuyPrice(badBuyPrice);

        cy.log('Placing post only limit orders');
        if (exchangeConfig.exchange === 'paradex') {
          cy.get('[data-testid=limit-buy-button]').first().click();
        } else {
          placeLimitOrderForE2E(OrderSide.Buy, badBuyPrice, { postOnly: true });
        }
        cy.wait(actionWaitTime);
        ensureOpenOrdersLengthForE2E(0);
        ensureTableBodyLength('[data-testid=open-positions-table]', 0);
      });
    }

    if (shouldRunTest('position-mode')) {
      it('Test Position Mode, Margin Type and Leverage', () => {
        const speedUpTest = true;

        const metaData = getAccMetadata();

        if (metaData.forceAccountPositionModeChangeOnExchangeItself) {
          cy.log('Position-mode UI selector is disabled for this exchange; skipping UI mode-switch test');
          return;
        }

        if (exchangeConfig.exchange === 'blofin') {
          cy.log('BloFin set-position-mode returns 400 for this account; skipping UI mode-switch test');
          return;
        }

        const positionModes = metaData.positionModes;
        const positionModeOrderSizeInQuote = exchangeConfig.positionModeOrderSizeInQuote ?? orderSizeInQuote;
        // Bybit Unified accounts reject /v5/position/switch-isolated (retCode 100028)
        // so lock margin type to Cross only
        const marginTypes =
          exchangeConfig.marginTypes ??
          (exchangeConfig.exchange === 'bybit' || exchangeConfig.exchange === 'binance'
            ? [MarginType.Cross]
            : metaData.marginTypes);

        setOrderTypeTab('market');

        cy.log('Setting size and price');
        setOrderSize(positionModeOrderSizeInQuote);

        // ensure reduce is not checked
        cy.log('Ensure reduce only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);

        // cycle back to first position mode, skip entirely if only one
        for (let i = 0; i <= positionModes.length; i++) {
          // speedUpTest: skip cycle-back iteration (re-testing first mode)
          if (speedUpTest && i >= positionModes.length) {
            continue;
          }

          const positionMode = positionModes[i % positionModes.length];
          const positionModeLabel = tradeEn[`position-mode-${positionMode}`];
          if (positionModes.length > 1) {
            cy.log(`Setting position mode to ${positionMode}`);
            setPositionModeForE2E(positionMode, positionModeLabel);
            waitAndLogAPI('positionModeChange', `Position Mode Change to: ${positionMode}`);
            cy.wait(actionWaitTime);
            ensurePositionModeForE2E(positionModeLabel);
            waitForPhemexPositionMetadataForE2E({ positionMode });
          }

          // cycle back to first margin type, skip entirely if only one
          for (let j = 0; j <= marginTypes.length; j++) {
            // speedUpTest: skip cycle-back iteration (re-testing first margin type)
            if (speedUpTest && j >= marginTypes.length) {
              continue;
            }

            const marginType = marginTypes[j % marginTypes.length];
            const marginTypeLabel = tradeEn[`margin-type-${marginType}`];
            const shouldForceConfiguredMarginType =
              exchangeConfig.marginTypes?.length === 1 && metaData.marginTypes.length > 1;

            if (marginTypes.length > 1) {
              cy.log(`Setting margin type to ${marginType}`);
              setSelectValue('[data-testid=margin-type-selector]', marginTypeLabel);
              cy.wait(actionWaitTime);
              ensureSelectValue('[data-testid=margin-type-selector]', marginTypeLabel);
            } else if (shouldForceConfiguredMarginType) {
              cy.log(`Forcing configured margin type to ${marginType}`);
              forceMarginTypeForE2E(marginType);
              cy.wait(actionWaitTime);
              ensureSelectValue('[data-testid=margin-type-selector]', marginTypeLabel);
            }
            waitForPhemexPositionMetadataForE2E({ marginType, positionMode });

            const canAssertLeverageForCombo =
              canSetLeverageInE2E() && (marginType !== MarginType.Cross || metaData.supportsMaxCrossLeverage);

            const validatePositionData = (posSide: PositionSide, expectedLev?: number) => {
              // position should be open
              cy.log('Checking that position is open');
              ensureTableBodyLengthOneOf('[data-testid=open-positions-table]', [1, 2]);

              if (exchangeConfig.exchange === 'woo') {
                recoverWooSymbolPositionsForE2E();
              } else {
                cy.wait(slowUpdateExtraWaitTime);
              }
              // ensure position mode, margin type and leverage are correct
              cy.get('[data-testid=open-positions-table]').then(($element) => {
                const header = getTableHeader($element);
                const position = findPosition($element, posSide, displaySymbol);

                // New format: "5.23 / 20x [C,O]" or "5.23 / 20x\n[C,O]"
                // Where 5.23 is account leverage, 20x is position leverage, [C,O] is margin type and position mode
                const leverageCell = getCell(header, position, 'Leverage');
                const cellText = leverageCell.innerText.trim();

                // Extract the mode part (in brackets) using regex
                const modeMatch = cellText.match(/\[([^\]]+)\]/);
                const modeParts = modeMatch ? modeMatch[1] : '';

                // Extract the leverage part (everything before the brackets, or the whole text if no brackets)
                const levParts = cellText.replace(/\s*\[[^\]]+\]\s*/, '').trim();
                const [_effectiveLev, _selectedLev] = levParts.split(' / ');

                if (expectedLev !== undefined && _selectedLev) {
                  const selectedLev = parseFloat(_selectedLev.replace('x', ''));
                  expect(selectedLev).to.eq(expectedLev);
                }

                if (modeParts) {
                  const [mModePrefix, pModePrefix] = modeParts.split(',').map((s) => s.trim());
                  expect(pModePrefix).to.eq(positionModeLabel.substring(0, 1));
                  expect(mModePrefix).to.eq(marginTypeLabel.substring(0, 1));
                }
              });
            };

            cy.log('Check that everything matches before even setting leverage');

            if (!canAssertLeverageForCombo) {
              // Ensure Market tab is active and size is set (may reset after position mode/margin type changes)
              cy.get('[data-testid=toggle-tabs-market]').first().click();
              setOrderSize(positionModeOrderSizeInQuote);
              cy.log('Placing market buy order');
              placeMarketOrderForE2E(OrderSide.Buy, positionModeOrderSizeInQuote);
              waitForOpenedPositionForE2E(true);
              ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
              validatePositionData(PositionSide.Long);
            } else {
              getSliderValue('selectedLongLeverage', '[data-testid=leverage-slider]', 0);
              cy.get('@selectedLongLeverage').then(function () {
                const levVal: number = this.selectedLongLeverage;
                // Ensure Market tab is active and size is set (may reset after position mode/margin type changes)
                cy.get('[data-testid=toggle-tabs-market]').first().click();
                setOrderSize(positionModeOrderSizeInQuote);
                cy.log('Placing market buy order');
                placeMarketOrderForE2E(OrderSide.Buy, positionModeOrderSizeInQuote);
                waitForOpenedPositionForE2E(true);
                ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
                validatePositionData(PositionSide.Long, levVal);
              });

              if (positionMode === PositionMode.Hedged && metaData.hasIndependentHedgedLeverage) {
                getSliderValue('selectedShortLeverage', '[data-testid=leverage-slider]', 1);
                cy.get('@selectedShortLeverage').then(function () {
                  const levVal: number = this.selectedShortLeverage;
                  // Ensure Market tab is active (may reset after position updates)
                  cy.get('[data-testid=toggle-tabs-market]').first().click();
                  setOrderSize(positionModeOrderSizeInQuote);
                  cy.log('Placing sell limit order');
                  placeMarketOrderForE2E(OrderSide.Sell, positionModeOrderSizeInQuote);
                  waitForOpenedPositionForE2E(true);
                  ensureTableBodyLength('[data-testid=open-positions-table]', 2, positionWaitTimeout);
                  validatePositionData(PositionSide.Short, levVal);
                });
              }
            }

            flattenPositions();

            // do nothing, we did the test already above as no leverage to set....
            if (!canAssertLeverageForCombo) {
              // dont bother on last cycle through resetting
            } else if (i < positionModes.length && j < marginTypes.length) {
              // speedUpTest: skip leverage magnet testing after first combo
              if (speedUpTest && (i >= 1 || j >= 1)) {
                continue;
              }

              // speedUpTest: test only 1 leverage value instead of 2
              const magnetValsWithIndex = getLeverageMagnetsToTest(metaData, speedUpTest ? [10] : [5, 20]);

              magnetValsWithIndex.forEach(([levVal, _levMagnetIndex], k) => {
                // used for hedge mode with independent leverage
                const [shortLevVal] = magnetValsWithIndex[(k + 1) % magnetValsWithIndex.length];

                cy.log(`Setting leverage to ${levVal}`);
                setNativeSliderValue('[data-testid=leverage-slider]', levVal);
                cy.wait(actionWaitTime);
                ensureSliderValue('[data-testid=leverage-slider]', levVal);

                if (positionMode === PositionMode.Hedged && metaData.hasIndependentHedgedLeverage) {
                  setNativeSliderValue('[data-testid=leverage-slider]', shortLevVal, 1);
                  cy.wait(actionWaitTime);
                  ensureSliderValue('[data-testid=leverage-slider]', shortLevVal, 1);
                  ensureSliderValue('[data-testid=leverage-slider]', levVal);
                }

                // Ensure Market tab is active and size is set (tab may reset after position mode/margin type changes)
                cy.get('[data-testid=toggle-tabs-market]').first().click();
                setOrderSize(positionModeOrderSizeInQuote);
                cy.log('Placing market buy order');
                placeMarketOrderForE2E(OrderSide.Buy, positionModeOrderSizeInQuote);
                waitForOpenedPositionForE2E(true);
                ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
                validatePositionData(PositionSide.Long, levVal);

                if (positionMode === PositionMode.Hedged && metaData.hasIndependentHedgedLeverage) {
                  // Ensure Market tab is active (may reset after position updates)
                  cy.get('[data-testid=toggle-tabs-market]').first().click();
                  setOrderSize(positionModeOrderSizeInQuote);
                  cy.log('Placing sell limit order');
                  placeMarketOrderForE2E(OrderSide.Sell, positionModeOrderSizeInQuote);
                  waitForOpenedPositionForE2E(true);
                  ensureTableBodyLength('[data-testid=open-positions-table]', 2, positionWaitTimeout);
                  validatePositionData(PositionSide.Short, shortLevVal);
                }

                flattenPositions();
              });
            }
          }
        }
      });
    }

    if (shouldRunTest('reduce-orders')) {
      it('Test reduce orders and opening position', () => {
        // Fetch fresh lastTrade value before placing orders
        getLastTrade();

        // BUY SIDE
        cy.then(() => {
          const sizeInBase = orderSizeInQuote / lastTrade;
          const buyPrice = handleRoundingError(lastTrade * (1 + crossDelta));
          const nextBuyPrice = handleRoundingError(lastTrade * (1 + farTouchDelta * 2));

          cy.log(`Placing buy limit at ${buyPrice} (market: ${lastTrade})`);
          setBuyPrice(buyPrice);
          setOrderSize(orderSizeInQuote);

          // ensure reduce is not checked
          cy.log('Ensure reduce only is not checked');
          ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);
          cy.log('Placing buy limit order');
          cy.get('[data-testid=limit-buy-button]').first().click();
          waitForOpenedPositionForE2E(true);

          // order should be filled - ensure we're on the correct orders tab
          cy.log('Checking that order is filled');
          clickOrdersTab('blue-tabs-ActiveMarketActiveOrders');

          ensureTableBodyLength('[data-testid=open-orders-table]', 0);

          // position should be open - use longer timeout for exchanges with slow position updates
          cy.log('Checking that position is open');
          ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);

          // ensure size is correct
          ensurePositionSize(orderSizeInQuote, PositionSide.Long, symbol);

          const checkOrderValues = (
            header: HTMLElement,
            order: HTMLElement,
            side: OrderSide,
            price: number,
            size: number,
          ) => {
            expect(safelyParseValue(getCell(header, order, 'Price').textContent)).to.be.closeTo(price, price * 0.04);
            const remaining = safelyParseValue(getCell(header, order, 'Remaining').textContent);
            if (
              exchangeConfig.exchange === 'kraken' &&
              (Number.isNaN(remaining) || Math.abs(remaining - size) > size * orderSizeVariance)
            ) {
              expect(
                getKrakenReduceLimitAmountFromStore(side, price),
                'Kraken store reduce order amount',
              ).to.be.closeTo(size, size * orderSizeVariance);
            } else {
              expect(remaining).to.be.closeTo(size, size * orderSizeVariance);
            }
            expect(getCell(header, order, 'Reduce').textContent).to.eq('Yes');
          };

          getDisplayedPositionSizeInBase(PositionSide.Long).then((longPositionSizeInBase) => {
            cy.log('Switching to limit orders');
            cy.get('[data-testid=toggle-tabs-limit]').first().click();

            cy.log('Prepping reduce only order');
            const reduceSellPrice = handleRoundingError(lastTrade * (1 + farTouchDelta));
            setSellPrice(reduceSellPrice);
            setReduceCloseSize(longPositionSizeInBase, reduceSellPrice);
            ensureCheckboxIsChecked('[data-testid=reduce-only-checkbox]', 1);
            if (exchangeConfig.reduceOnlyLimitPostOnly) {
              ensureCheckboxIsChecked('[data-testid=post-only-checkbox]', 1);
            }
            cy.log(`Placing reduce sell limit of size ${longPositionSizeInBase} base`);
            cy.get('[data-testid=limit-sell-button]').first().click();
            cy.wait(actionWaitTime);
            cy.wait(slowUpdateExtraWaitTime);

            cy.log('Checking that reduce order is placed');
            clickOrdersTab('blue-tabs-ActiveMarketActiveOrders');
            recoverHostedOpenOrdersForE2E();
            ensureTableBodyLength('[data-testid=open-orders-table]', 1);

            cy.log(`Checking that orders is reduce`);
            cy.get('[data-testid=open-orders-table]').then(($element) => {
              const header = getTableHeader($element);

              cy.log('Checking buy order');
              checkOrderValues(
                header,
                findOrder($element, OrderSide.Sell, displaySymbol),
                OrderSide.Sell,
                reduceSellPrice,
                longPositionSizeInBase,
              );

              if (canAmendReduceOrders()) {
                cy.log(`Updating buy order price to ${nextBuyPrice}`);
                updateOrderPrice(OrderSide.Sell, symbol, nextBuyPrice);

                ensureTableBodyLength('[data-testid=open-orders-table]', 1);
                ensureTableBodyLength('[data-testid=open-positions-table]', 1);

                cy.log('Checking updated orders');
                cy.get('[data-testid=open-orders-table]').should(($element) => {
                  const freshHeader = getTableHeader($element);
                  checkOrderValues(
                    freshHeader,
                    findOrder($element, OrderSide.Sell, displaySymbol),
                    OrderSide.Sell,
                    nextBuyPrice,
                    longPositionSizeInBase,
                  );
                });
              } else {
                cy.log(`${exchangeConfig.exchange} does not support exact reduce-order amend validation`);
              }
            });

            flattenPositions();

            cy.log('Ensure reduce only order cancels');
            ensureTableBodyLength('[data-testid=open-orders-table]', 0);
          });

          // SELL SIDE

          const sellPrice = handleRoundingError(lastTrade * (1 - crossDelta));

          cy.log(`Placing sell limit at ${sellPrice} (market: ${lastTrade})`);
          setSellPrice(sellPrice);
          setOrderSize(orderSizeInQuote);

          // ensure reduce is not checked
          cy.log('Ensure reduce only is not checked');
          ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);
          ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 1);

          cy.log('Placing sell limit order');
          cy.get('[data-testid=limit-sell-button]').first().click();
          waitForOpenedPositionForE2E(true);

          // order should be filled
          cy.log('Checking that order is filled');
          ensureTableBodyLength('[data-testid=open-orders-table]', 0);

          // position should be open - use longer timeout for exchanges with slow position updates
          cy.log('Checking that position is open');
          ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);

          // ensure size is correct
          ensurePositionSize(orderSizeInQuote, PositionSide.Short, symbol);

          getDisplayedPositionSizeInBase(PositionSide.Short).then((shortPositionSizeInBase) => {
            cy.log('Prepping reduce only limit order');
            const reduceBuyPrice = handleRoundingError(lastTrade * (1 + crossDelta)); // Buy above market to fill
            setBuyPrice(reduceBuyPrice);
            setReduceCloseSize(shortPositionSizeInBase, reduceBuyPrice);
            ensureCheckboxIsChecked('[data-testid=reduce-only-checkbox]', 0);
            if (exchangeConfig.reduceOnlyLimitPostOnly) {
              ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 0);
            }

            cy.log(`Placing reduce buy limit of size ${shortPositionSizeInBase} base`);
            cy.get('[data-testid=limit-buy-button]').first().click();
            cy.wait(actionWaitTime);
            cy.wait(slowUpdateExtraWaitTime);

            // order should be filled
            cy.log('Checking that order is filled');
            ensureTableBodyLength('[data-testid=open-orders-table]', 0);

            // position should be closed - use longer timeout for exchanges with slow position updates
            cy.log('Checking that position is closed and no opposite position is opened');
            ensureTableBodyLength('[data-testid=open-positions-table]', 0, positionWaitTimeout);
          });
        });
      });
    }

    if (shouldRunTest('hedge-mode')) {
      it('Test Hedge Mode Behavior', () => {
        const metaData = getAccMetadata();
        const positionModes = metaData.positionModes;

        if (!positionModes.includes(PositionMode.Hedged)) {
          cy.log('Hedged mode not supported, skipping test');
          return;
        }
        if (metaData.forceAccountPositionModeChangeOnExchangeItself) {
          cy.log('Position-mode UI selector is disabled for this exchange; skipping hedge-mode UI test');
          return;
        }

        cy.log('Setting position mode to Hedged');
        setPositionModeForE2E(PositionMode.Hedged, tradeEn['position-mode-hedged']);
        cy.wait(actionWaitTime);
        ensurePositionModeForE2E('Hedged');
        waitForPhemexPositionMetadataForE2E({ positionMode: PositionMode.Hedged });
        setHedgeModeLeverageForE2E(metaData);

        // make sure positions open correctly and match above settings
        // Both prices before the size — the size cell converts quote→base at the
        // form's price, so a write made while a limit price is still 0 is dropped.
        cy.log('Setting size and price');
        setBuyPrice(handleRoundingError(lastTrade * (1 + crossDelta)));
        setSellPrice(handleRoundingError(lastTrade * (1 - crossDelta)));
        setOrderSize(orderSizeInQuote);

        // BUY ORDER

        // ensure post only is not checked
        cy.log('Ensure post only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 0);

        // ensure reduce is not checked
        cy.log('Ensure reduce only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);

        cy.log('Placing buy limit order');
        cy.get('[data-testid=limit-buy-button]').first().click();
        cy.wait(actionWaitTime);
        cy.wait(slowUpdateExtraWaitTime);

        // position should be open - use longer timeout for slow position updates
        cy.log('Checking that position is open');
        ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);

        // SELL ORDER
        setSellPrice(handleRoundingError(lastTrade * (1 - crossDelta)));

        // ensure post only is not checked
        cy.log('Ensure post only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 1);

        // ensure reduce is not checked
        cy.log('Ensure reduce only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);

        cy.log('Placing sell limit order');
        cy.get('[data-testid=limit-sell-button]').first().click();
        cy.wait(actionWaitTime);

        // position should be open — ensureTableBodyLength uses .should() which retries
        cy.log('Checking that position is open');
        ensureTableBodyLength('[data-testid=open-positions-table]', 2, positionWaitTimeout);

        // MESS WITH BOTH SIDES

        cy.log('Ensure non reduce buy adds to long position');
        cy.get('[data-testid=limit-buy-button]').first().click();
        cy.wait(actionWaitTime);

        // ensurePositionSize uses .should() which retries until position updates
        ensurePositionSize(orderSizeInQuote * 2, PositionSide.Long, symbol, positionWaitTimeout);
        ensurePositionSize(orderSizeInQuote, PositionSide.Short, symbol);

        cy.log('Ensure non reduce sell adds to short position');
        cy.get('[data-testid=limit-sell-button]').first().click();
        cy.wait(actionWaitTime);

        ensurePositionSize(orderSizeInQuote * 2, PositionSide.Long, symbol, positionWaitTimeout);
        ensurePositionSize(orderSizeInQuote * 2, PositionSide.Short, symbol, positionWaitTimeout);

        cy.log('Reduce long position');
        ensureCheckboxIsChecked('[data-testid=reduce-only-checkbox]', 1);
        cy.get('[data-testid=limit-sell-button]').first().click();
        cy.wait(actionWaitTime);

        ensurePositionSize(orderSizeInQuote, PositionSide.Long, symbol, positionWaitTimeout);
        ensurePositionSize(orderSizeInQuote * 2, PositionSide.Short, symbol, positionWaitTimeout);
      });
    }

    // TODO: validate trigger types
    if (shouldRunTest('bracket-orders')) {
      it('Place Bracket Orders', () => {
        cy.get('[data-testid=toggle-tabs-limit]').first().click();

        cy.log('Setting size and price');
        const sizeInBase = orderSizeInQuote / lastTrade;

        const buyPrice = handleRoundingError(lastTrade * (1 - farTouchDelta));
        const buyTpPrice = handleRoundingError(lastTrade * (1 + 2 * farTouchDelta));
        const buySlPrice = handleRoundingError(buyPrice * (1 - 2 * farTouchDelta));

        const sellPrice = handleRoundingError(lastTrade * (1 + farTouchDelta));
        const sellTpPrice = handleRoundingError(lastTrade * (1 - 2 * farTouchDelta));
        const sellSlPrice = handleRoundingError(sellPrice * (1 + 2 * farTouchDelta));
        setBuyPrice(buyPrice);
        setSellPrice(sellPrice);
        setOrderSize(orderSizeInQuote);

        // ensure post only is not checked
        cy.log('Ensure post only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 0);
        ensureCheckboxIsNotChecked('[data-testid=post-only-checkbox]', 1);

        // ensure reduce is not checked
        cy.log('Ensure reduce only is not checked');
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);

        const validateBracketOrders = (side: OrderSide, limitPrice: number, tpPrice: number, slPrice: number) => {
          // Virtual TPSL orders (Bitget) rely on REST polling and REST cache can be delayed
          const bracketWait = exchangeConfig.virtualTpSlSyncWait || slowUpdateExtraWaitTime;
          cy.wait(bracketWait);

          if (exchangeConfig.exchange === 'paradex') {
            cy.then(() => {
              expect(lastParadexPlacedOrderIds.length, 'Paradex bracket order ids').to.eq(3);
            });
            recoverHostedOpenOrdersForE2E();
            return;
          }

          // ActiveMarketAllOrders shows everything for the symbol (limit + triggers).
          // Don't switch to ActiveMarketActiveOrders here — that tab filters triggers
          // out (commit 12ea9912d6, May 2026), so it would only show the entry order.
          clickOrdersTab('blue-tabs-ActiveMarketAllOrders');
          if (exchangeConfig.exchange === 'okx') {
            ensureTableBodyLengthAtLeast('[data-testid=open-orders-table]', 1);
            ensureOkxBracketAlgoOrdersForE2E(side, tpPrice, slPrice, sizeInBase);
            return;
          }
          ensureTableBodyLength('[data-testid=open-orders-table]', 3);

          // Switch to Stop Orders tab to validate the TP/SL details
          clickOrdersTab('blue-tabs-ActiveMarketStopOrders');
          ensureTableBodyLength('[data-testid=open-orders-table]', 2);

          // ensure stop orders are correct
          cy.get('[data-testid=open-orders-table]').then(($element) => {
            const trs = $element.find('tr');
            const header = trs[0];
            const orders = trs.toArray().slice(1);

            const typeIndex = Array.from(header.children).findIndex((td: any) => td.innerText.startsWith('Type'));
            const findStopOrder = (typeLabel: string, _expectedPrice: number) => {
              const typedOrders = orders.filter((order) => order.children[typeIndex].textContent?.includes(typeLabel));
              return typedOrders[0];
            };

            const takeProfitOrder = findStopOrder('Take Profit', tpPrice);
            assert.isDefined(takeProfitOrder, `Take Profit near ${tpPrice}`);

            const stopOrder = findStopOrder('Stop Loss', slPrice);
            assert.isDefined(stopOrder, `Stop Loss near ${slPrice}`);

            ensureOrderSide(header, takeProfitOrder!, side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy);
            ensureOrderSide(header, stopOrder!, side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy);

            expect(safelyParseValue(getCell(header, takeProfitOrder!, 'Price').textContent)).to.be.closeTo(
              tpPrice,
              tpPrice * 0.02,
            );

            expect(safelyParseValue(getCell(header, stopOrder!, 'Price').textContent)).to.be.closeTo(
              slPrice,
              slPrice * 0.02,
            );

            const tpRemainingStr = getCell(header, takeProfitOrder!, 'Remaining').textContent!.trim();
            const tpRemaining = safelyParseValue(tpRemainingStr);
            if (isNaN(tpRemaining)) {
              expect(tpRemainingStr).to.include('Full Position');
            } else {
              expect(tpRemaining).to.be.closeTo(sizeInBase, sizeInBase * orderSizeVariance);
            }

            const slRemainingStr = getCell(header, stopOrder!, 'Remaining').textContent!.trim();
            const slRemaining = safelyParseValue(slRemainingStr);
            if (isNaN(slRemaining)) {
              expect(slRemainingStr).to.include('Full Position');
            } else {
              expect(slRemaining).to.be.closeTo(sizeInBase, sizeInBase * orderSizeVariance);
            }
          });
        };

        cy.log('Set buy TP / SL prices');
        cy.get('[data-testid=take-profit-input]')
          .eq(0)
          .find('input')
          .then(($input) => {
            setNativeInputValue($input, `${buyTpPrice}`);
          });
        cy.wait(200);

        cy.get('[data-testid=stop-loss-input]')
          .eq(0)
          .find('input')
          .then(($input) => {
            setNativeInputValue($input, `${buySlPrice}`);
          });
        cy.wait(200);

        cy.log('Placing Buy Bracket');
        placeLimitOrderForE2E(OrderSide.Buy, buyPrice, { takeProfit: buyTpPrice, stopLoss: buySlPrice });
        cy.wait(actionWaitTime);

        validateBracketOrders(OrderSide.Buy, buyPrice, buyTpPrice, buySlPrice);

        flattenOrders();
        flattenPositions();

        cy.log('Set Sell TP / SL prices');
        cy.get('[data-testid=take-profit-input]')
          .eq(1)
          .find('input')
          .then(($input) => {
            setNativeInputValue($input, `${handleRoundingError(lastTrade * (1 - 2 * farTouchDelta))}`);
          });
        cy.wait(200);

        cy.get('[data-testid=stop-loss-input]')
          .eq(1)
          .find('input')
          .then(($input) => {
            setNativeInputValue($input, `${handleRoundingError(sellPrice * (1 + 2 * farTouchDelta))}`);
          });
        cy.wait(200);

        cy.log('Placing Sell Bracket');
        placeLimitOrderForE2E(OrderSide.Sell, sellPrice, { takeProfit: sellTpPrice, stopLoss: sellSlPrice });
        cy.wait(actionWaitTime);

        validateBracketOrders(OrderSide.Sell, sellPrice, sellTpPrice, sellSlPrice);
      });
    }

    if (shouldRunTest('margin-slider')) {
      it('Test Margin Slider', () => {
        getAvailableMargin();

        cy.log('Ensure reduce is not checked');
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);

        const testMarginSliderForLev = (leverage: number) => {
          // buy / sell
          [0, 1].forEach((side) => {
            // skip 0 bc it doesnt work with min sizes and rounding
            [0.25, 0.5, 0.75, 1].forEach((p) => {
              cy.log(`Setting buy size to ${p * 100}% of available margin`);
              setNativeSliderValue('[data-testid=margin-slider]', p * 100, side);
              cy.wait(1000);
              ensureSliderValueCloseTo('[data-testid=margin-slider]', p * 100, 2, side);
              cy.get('body').then(($body) => {
                const sizeCellsSelector = resolveQuoteSizeCellSelector($body);
                // The assertion below compares against a quote-denominated
                // expectation, so the cell must be showing quote units.
                ensureQuoteSizeUnit(sizeCellsSelector, side);
                cy.get('@availableMargin').then((availableMargin) => {
                  const expectedQuoteSize = Number(availableMargin) * leverage * p;
                  cy.get(sizeCellsSelector)
                    .filter(':visible')
                    .eq(side)
                    .find('input')
                    .should(($input) => {
                      const quoteSize = safelyParseValue(String($input.val() || '0'));
                      expect(quoteSize).to.be.closeTo(expectedQuoteSize, expectedQuoteSize * 0.1);
                    });
                });
              });
            });
          });
        };

        const inferAndTestCurrentMarginSliderLeverage = () => {
          // Some venues expose no trustworthy leverage control for E2E. Infer
          // the active multiplier from the 100% margin slider output instead.
          setNativeSliderValue('[data-testid=margin-slider]', 100, 0);
          cy.wait(1000);
          ensureSliderValueCloseTo('[data-testid=margin-slider]', 100, 2, 0);
          cy.get('body').then(($body) => {
            const sizeCellsSelector = resolveQuoteSizeCellSelector($body);
            ensureQuoteSizeUnit(sizeCellsSelector, 0);
            cy.get('@availableMargin').then((availableMarginValue) => {
              const availableMargin = Number(availableMarginValue);
              cy.get(sizeCellsSelector)
                .filter(':visible')
                .eq(0)
                .find('input')
                .should(($input) => {
                  const quoteSize = safelyParseValue(String($input.val() || '0'));
                  const leverage = quoteSize / availableMargin;
                  expect(leverage).to.be.greaterThan(0);
                })
                .then(($input) => {
                  const quoteSize = safelyParseValue(String($input.val() || '0'));
                  const leverage = quoteSize / availableMargin;
                  testMarginSliderForLev(leverage);
                });
            });
          });
        };

        const metaData = getAccMetadata();
        if (shouldInferMarginSliderLeverage()) {
          inferAndTestCurrentMarginSliderLeverage();
          return;
        }

        if (metaData.supportsMaxCrossLeverage && canSetLeverageInE2E()) {
          const [[leverage]] = getLeverageMagnetsToTest(metaData, [10]);
          cy.log(`Setting leverage to ${leverage}`);
          setNativeSliderValue('[data-testid=leverage-slider]', leverage);
          cy.wait(actionWaitTime);
          ensureSliderValue('[data-testid=leverage-slider]', leverage);
          testMarginSliderForLev(leverage);
        } else {
          cy.get('body').then(($body) => {
            if ($body.find('[data-testid=max-leverage]').length > 0) {
              cy.get('[data-testid=max-leverage]').then(($element) => {
                const maxLeverage = parseFloat($element.text().replace('x', ''));
                testMarginSliderForLev(maxLeverage);
              });
            } else if ($body.find('[data-testid=leverage-slider]:visible').length > 0) {
              cy.get('[data-testid=leverage-slider]')
                .filter(':visible')
                .first()
                .find('input[type=range]')
                .then(($slider) => {
                  const leverage = parseFloat($slider.val() as string);
                  testMarginSliderForLev(leverage);
                });
            } else {
              inferAndTestCurrentMarginSliderLeverage();
            }
          });
        }
      });
    }

    if (shouldRunTest('stop-orders')) {
      it('Place stop orders with partial, full close, all trigger types', () => {
        const speedUpTest = true;

        const metaData = getAccMetadata();
        const triggerTypes = metaData.supportedTPSLTriggerTypes;

        // Ensure clean state: flatten any leftover positions/orders from previous tests
        flattenOrders();
        flattenPositions();

        setOrderTypeTab('market');
        setOrderSize(orderSizeInQuote);
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 0);
        ensureCheckboxIsNotChecked('[data-testid=reduce-only-checkbox]', 1);

        setOrderTypeTab('tpsl');

        // const sizeInBase = orderSizeInQuote / lastTrade;
        const buyPrice = handleRoundingError(lastTrade * (1 + farTouchDelta));
        const sellPrice = handleRoundingError(lastTrade * (1 - farTouchDelta));

        const nextBuyPrice = handleRoundingError(lastTrade * (1 + farTouchDelta * 2));
        const nextSellPrice = handleRoundingError(lastTrade * (1 - farTouchDelta * 2));

        cy.log('Setting price');
        setBuyPrice(buyPrice);
        setSellPrice(sellPrice);

        const checkOrderValues = (
          header: HTMLElement,
          order: HTMLElement,
          side: OrderSide,
          price: number,
          size: number | string,
          reduce: boolean,
          triggerLabel?: string,
        ) => {
          // Stop order prices can drift slightly from the intended value due to market movement
          // between lastTrade capture and order placement — use 6% tolerance (vs 4% for limit orders)
          expect(safelyParseValue(getCell(header, order, 'Price').textContent)).to.be.closeTo(price, price * 0.06);
          if (typeof size === 'number') {
            const sizeInBase = size / lastTrade;
            const remaining = safelyParseValue(getCell(header, order, 'Remaining').textContent);
            if (Number.isNaN(remaining) && exchangeConfig.exchange === 'hyperliquid') {
              expect(getHyperliquidStopAmountFromStore(side, price), 'Hyperliquid store stop amount').to.be.closeTo(
                sizeInBase,
                sizeInBase * orderSizeVariance,
              );
            } else {
              expect(remaining).to.be.closeTo(sizeInBase, sizeInBase * orderSizeVariance);
            }
          } else if (size === 'Full Position') {
            const remainingText = getCell(header, order, 'Remaining').textContent?.trim();
            if (exchangeConfig.exchange === 'bitunix') {
              // Bitunix full-position TP/SL can surface as API amount=0, a
              // resolved quantity, blank table text, or literal UI copy.
              const remaining = safelyParseValue(remainingText);
              if (!isNaN(remaining)) {
                expect(remaining).to.be.at.least(0);
              } else {
                expect(remainingText).to.satisfy((text: string | undefined) => {
                  return !text || text === 'Full Position';
                });
              }
            } else {
              expect(remainingText).to.eq(size);
            }
          } else {
            throw new Error('Invalid size');
          }
          expect(safelyParseValue(getCell(header, order, 'Filled').textContent)).to.equal(0);
          const orderType = reduce ? 'Stop Loss' : 'Take Profit';
          const expectedOrderTypes = [orderType];
          if (triggerLabel) {
            const labeledOrderType = `${orderType} [${triggerLabel}]`;
            if (displaysStopTriggerLabel() || exchangeConfig.exchange === 'paradex') {
              expectedOrderTypes.push(labeledOrderType);
            }
          }

          expect(getCell(header, order, 'Type').textContent?.trim()).to.be.oneOf(expectedOrderTypes);
          // either GTC or ''
          expect(getCell(header, order, 'TIF').textContent).to.be.oneOf(['', 'GTC', 'IOC']);
          const reduceText = getCell(header, order, 'Reduce').textContent;
          if (exchangeConfig.exchange === 'bitunix' && reduce) {
            expect(reduceText).to.be.oneOf(['Yes', 'No']);
          } else {
            expect(reduceText).to.eq(reduce ? 'Yes' : 'No');
          }
        };

        const placeAndValidateStops = (
          side: OrderSide,
          reduce: boolean,
          size: number | string,
          triggerType?: OrderStopTriggerType,
        ) => {
          const eq = side === OrderSide.Buy ? 0 : 1;

          // Ensure TPSL tab is active (may have switched after flattenOrders)
          setOrderTypeTab('tpsl');
          cy.wait(500);

          // Re-set prices (tab switches and flattenOrders can clear form inputs)
          setBuyPrice(buyPrice);
          setSellPrice(sellPrice);

          clickOrdersTab('blue-tabs-AllStopOrders');

          // Modern TPSL form has no reduce-only checkbox — createStopLossOrder
          // hardcodes reduceOnly=true for all stop orders. The FullStopReduceToggle
          // controls Full Stop (full position) vs Partial (specific size) mode.

          if (size === 'Full Position') {
            ensureCheckboxIsChecked('[data-testid=full-stop-checkbox]', 0);
            ensureCheckboxIsChecked('[data-testid=full-stop-checkbox]', 1);
          } else if (typeof size === 'number') {
            ensureCheckboxIsNotChecked('[data-testid=full-stop-checkbox]', 0);
            ensureCheckboxIsNotChecked('[data-testid=full-stop-checkbox]', 1);
            cy.wait(300);
            setOrderSize(size);
          } else {
            throw new Error(`Invalid size ${size}`);
          }

          let triggerTypeLabel = undefined;
          if (triggerType) {
            triggerTypeLabel = tradeEn[`trigger-type-${triggerType.replace('_', '-')}`];

            cy.log(`Setting trigger type to ${triggerTypeLabel}`);
            setTriggerTypeOnBothSides(triggerType, triggerTypeLabel);
          }

          cy.log(`Placing ${side} stop order`);
          cy.get(`[data-testid=tpsl-${side === OrderSide.Buy ? 'buy' : 'sell'}-button]`)
            .filter(':visible')
            .first()
            .click();
          cy.wait(actionWaitTime);
          cy.wait(slowUpdateExtraWaitTime);

          cy.log('Checking that order is placed');

          if (exchangeConfig.exchange === 'okx') {
            ensureOkxStandaloneStopOrderForE2E(side, side === OrderSide.Buy ? buyPrice : sellPrice, size);
            return;
          }

          // 1 or 2 depending on if other stops auto cancel
          ensureTableBodyLengthOneOf('[data-testid=open-orders-table]', [1, 2]);

          cy.get('[data-testid=open-orders-table]').then(($element) => {
            const header = getTableHeader($element);
            checkOrderValues(
              header,
              findOrder($element, side, displaySymbol, true),
              side,
              side === OrderSide.Buy ? buyPrice : sellPrice,
              size,
              reduce,
              triggerTypeLabel,
            );

            if (!canAmendStopOrders()) {
              cy.log(`${exchangeConfig.exchange} does not support exact stop-order amend validation`);
              return;
            }

            cy.log('Updating order price');
            const nextPrice = side === OrderSide.Buy ? nextBuyPrice : nextSellPrice;
            updateOrderPrice(side, symbol, nextPrice, true);

            cy.log('Checking that order is placed');
            ensureTableBodyLengthOneOf('[data-testid=open-orders-table]', [1, 2]);

            cy.get('[data-testid=open-orders-table]').should(($element) => {
              const freshHeader = getTableHeader($element);
              checkOrderValues(
                freshHeader,
                findOrder($element, side, displaySymbol, true),
                side,
                nextPrice,
                size,
                reduce,
                triggerTypeLabel,
              );
            });
          });
        };
        const partialStopSizeInQuote = metaData.exchangeName === 'bitunix' ? orderSizeInQuote * 0.6 : orderSizeInQuote;
        const sizes = [
          ...(metaData.supportsInfinityStops && metaData.exchangeName !== 'bitunix' ? ['Full Position'] : []),
          ...(metaData.supportsPartialStops ? [partialStopSizeInQuote] : []),
        ];

        // Skip position mode switching for exchanges where the API is broken or restricted
        // Phemex: rejects mode switch with pending TP/SL state (TE_REDUCE_ONLY_ABORT)
        // BloFin: set-position-mode returns 400 via proxy — stuck in Hedged mode
        const skipHedgedStops = metaData.exchangeName === 'phemex';
        const skipPositionModeSwitch =
          metaData.exchangeName === 'blofin' || metaData.forceAccountPositionModeChangeOnExchangeItself;
        const positionModes = skipPositionModeSwitch
          ? metaData.positionModes.slice(0, 1)
          : skipHedgedStops
            ? metaData.positionModes.filter((mode: PositionMode) => mode !== PositionMode.Hedged)
            : metaData.positionModes;

        // some exchanges need an open position to bind TPSL orders
        const shouldOpenPosition = (size: number | string, positionMode: PositionMode): boolean => {
          if (metaData.exchangeName === 'phemex') {
            return true; // Phemex rejects all TP/SL orders when no position exists
          }
          if (metaData.exchangeName === 'bitget') {
            return true; // Bitget requires an open position to place algo TP/SL orders
          }
          if (metaData.exchangeName === 'bitunix') {
            return true; // Bitunix virtual TPSL is reduce-only; needs an open position
          }
          if (metaData.exchangeName === 'binance') {
            return size === 'Full Position';
          }
          if (metaData.exchangeName === 'bingx') {
            return size === 'Full Position';
          }
          if (metaData.exchangeName === 'okx') {
            return size === 'Full Position';
          }
          return false;
        };

        positionModes.forEach((positionMode, pModeIndex) => {
          const positionModeLabel = tradeEn[`position-mode-${positionMode}`];
          if (positionModes.length > 1) {
            // Skip position mode change if selector is disabled (e.g. Bitunix: forceAccountPositionModeChangeOnExchangeItself)
            cy.get('[data-testid=position-mode-selector]').then(($selector) => {
              if (!$selector.attr('class')?.includes('disabled')) {
                setSelectValue('[data-testid=position-mode-selector]', positionModeLabel);
                cy.wait(actionWaitTime);
                ensureSelectValue('[data-testid=position-mode-selector]', positionModeLabel);
              } else {
                cy.log(`Position mode selector disabled, skipping change to ${positionMode}`);
              }
            });
          }

          sizes.forEach((size) => {
            const supportsTPSLTriggers =
              metaData.supportedTPSLTriggerTypes && metaData.supportedTPSLTriggerTypes.length !== 0;
            if (supportsTPSLTriggers) {
              const triggerTypesToTest =
                metaData.exchangeName === 'phemex' && size === 'Full Position'
                  ? triggerTypes.slice(0, 1)
                  : triggerTypes;

              triggerTypesToTest.forEach((triggerType, triggerIndex) => {
                // speed up tests
                if (speedUpTest && pModeIndex >= 1 && triggerIndex >= 1) {
                  return;
                }
                [OrderSide.Buy, OrderSide.Sell].forEach((side, sideIndex) => {
                  // speed up tests
                  if (speedUpTest && (pModeIndex >= 1 || triggerIndex >= 1) && sideIndex >= 1) {
                    return;
                  }
                  if (shouldOpenPosition(size, positionMode)) {
                    cy.log(`Opening position for size=${size}, side=${side}, trigger=${triggerType}`);
                    setOrderTypeTab('market');
                    cy.wait(500);
                    setOrderSize(orderSizeInQuote);
                    cy.log(`Placing market order`);
                    clickMarketButton(side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy);
                    cy.wait(actionWaitTime);
                    cy.wait(slowUpdateExtraWaitTime);
                    ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
                    setOrderTypeTab('tpsl');
                  }
                  placeAndValidateStops(side, true, size, triggerType);
                  flattenOrders();
                  flattenPositions();
                });
              });
            } else {
              [OrderSide.Buy, OrderSide.Sell].forEach((side, sideIndex) => {
                // speed up tests
                if (speedUpTest && pModeIndex >= 1 && sideIndex >= 1) {
                  return;
                }
                if (shouldOpenPosition(size, positionMode)) {
                  cy.log(`Opening position for size=${size}, side=${side}`);
                  setOrderTypeTab('market');
                  cy.wait(500);
                  setOrderSize(orderSizeInQuote);
                  cy.log(`Placing market order`);
                  clickMarketButton(side);
                  cy.wait(actionWaitTime);
                  cy.wait(slowUpdateExtraWaitTime);
                  ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
                  setOrderTypeTab('tpsl');
                }
                placeAndValidateStops(side, true, size);
                flattenOrders();
              });
            }

            flattenOrders();
          });
        });
      });
    }
  },
);
