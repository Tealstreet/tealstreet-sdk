import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';
import type { ParadexSDKAccount } from './paradex.sdk';

import rateLimit from 'axios-rate-limit';
import sumBy from 'lodash/sumBy';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalOrder,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderHistoryOptions,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  PublicTrade,
  Ticker,
  timeframeToSeconds,
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { isRegularClientOrderShape, patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './paradex.api';
import { mapParadexFillToIncomeRecords } from './paradex.income';
import { mapParadexHistoricalOrder } from './paradex.order-history';
import { initializeParadexSDK, ParadexJWTManager, ParadexRequestSigner } from './paradex.sdk';
import {
  ACCOUNT_RESPONSE,
  BATCH_CANCEL_ORDER_RESPONSE,
  BATCH_ORDER_RESPONSE,
  ENDPOINTS,
  isParadexTriggerOrderType,
  MARGIN_RESPONSE,
  MARKET_RESPONSE,
  ORDER_RESPONSE,
  ORDER_SIDE,
  paradexTimeInForce,
  ORDER_TYPE,
  ORDERBOOK_RESPONSE,
  POSITION_RESPONSE,
  TICKER_RESPONSE,
} from './paradex.types';
import { ParadexPrivateWebSocket } from './paradex.ws-private';
import { ParadexPublicWebSocket } from './paradex.ws-public';

const PRIVATE_HISTORY_AUTH_RETRY_DELAYS_MS = [500, 1500] as const;

class ParadexPrivateHistoryAuthNotReadyError extends Error {
  readonly originalError?: unknown;

  constructor(operation: string, originalError?: unknown) {
    const detail = originalError instanceof Error && originalError.message ? `: ${originalError.message}` : '';
    super(`Paradex private auth not ready for ${operation}${detail}`);
    this.name = 'ParadexPrivateHistoryAuthNotReadyError';
    this.originalError = originalError;
  }
}

const isParadexPrivateHistoryAuthNotReadyError = (err: unknown): err is ParadexPrivateHistoryAuthNotReadyError =>
  err instanceof ParadexPrivateHistoryAuthNotReadyError;

// The errors this file wraps can be circular, and a log line must never replace the error it describes.
const describeError = (err: unknown): string => {
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
};

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const isPositiveFinitePrice = (price: number | undefined): price is number =>
  typeof price === 'number' && Number.isFinite(price) && price > 0;

const normalizePositiveFraction = (value: unknown): number | undefined => {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) return undefined;
  return parsed > 1 ? parsed / 100 : parsed;
};

const roundPriceToStep = (price: number, priceStep: number, direction: 'up' | 'down') => {
  const step = Number.isFinite(priceStep) && priceStep > 0 ? priceStep : 0.000001;
  const ratio = price / step;
  const roundedRatio = direction === 'up' ? Math.ceil(ratio - Number.EPSILON) : Math.floor(ratio + Number.EPSILON);
  return adjust(roundedRatio * step, step);
};

/** Request config carrying the payloadSigner's headers, or nothing when it returned none. */
const signerHeaders = (headers?: Record<string, string>) => (headers ? [{ headers }] : []);

export class ParadexExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Paradex', /^.{1,64}$/, {
    mint: () => `ts_${uuid()}`,
    isHonorableShape: (opts) => isRegularClientOrderShape(opts) && opts.close !== true,
  });

  /** Journal records: this venue has no reported closed axis, so round trips fold from fills. */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) =>
    this.foldClosedPositionRecords(opts, { symbolScoped: true });

  xhr: Axios;

  publicWebsocket: ParadexPublicWebSocket;
  privateWebsocket: ParadexPrivateWebSocket;

  // Paradex SDK integration
  sdkAccount: ParadexSDKAccount | null = null;
  sdkInitPromise: Promise<void> | null = null;
  requestSigner: ParadexRequestSigner | null = null;
  jwtManager: ParadexJWTManager | null = null;

  // Leverage settings per market (symbol -> leverage)
  // Stores leverage separately since Paradex doesn't return leverage for zero-size positions
  private leverageSettings = new Map<string, number>();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 10 });

    this.publicWebsocket = new ParadexPublicWebSocket(this);
    this.privateWebsocket = new ParadexPrivateWebSocket(this);

    // Initialize SDK asynchronously if Paradex address is provided
    if (opts.paradexAddress) {
      this.sdkInitPromise = this.initializeSDK();
      // The init runs fire-and-forget here; start() awaits sdkInitPromise and
      // surfaces any failure. Attach a no-op catch so that an exchange that is
      // constructed but never started (e.g. unit tests with no real creds)
      // doesn't trigger an unhandled promise rejection when init fails.
      void this.sdkInitPromise.catch(() => {});
    }
  }

  /**
   * Paradex auth uses key + paradexAddress + subkeyAddress (no secret required).
   * Override BaseExchange default which assumes a secret is mandatory.
   */
  get isPublicDataOnly(): boolean {
    if (this.options.publicDataOnly === true) {
      return true;
    }
    return !this.options.key || !this.options.paradexAddress || !this.options.subkeyAddress;
  }

  private async initializeSDK() {
    try {
      // Verify required credentials for subkey authentication
      if (!this.options.key) {
        throw new Error('Missing subkey private key (key field) for Paradex authentication');
      }
      if (!this.options.paradexAddress) {
        throw new Error('Missing Paradex address for Paradex authentication');
      }
      if (!this.options.subkeyAddress) {
        throw new Error('Missing subkey address for Paradex authentication');
      }

      this.log('Initializing Paradex SDK...', LogSeverity.Debug);
      this.sdkAccount = await initializeParadexSDK(this.options);

      if (this.sdkAccount) {
        this.log(
          `Paradex SDK initialized successfully - Account: ${this.sdkAccount.account.address}`,
          LogSeverity.Info,
        );

        // Initialize request signer for REST API authentication
        this.requestSigner = new ParadexRequestSigner(this.sdkAccount, this.options);
        this.log(
          `Request signer initialized for account: ${this.requestSigner.getAccountAddress()}`,
          LogSeverity.Debug,
        );

        // Initialize JWT manager for WebSocket authentication with subkey
        this.jwtManager = new ParadexJWTManager(
          this.sdkAccount,
          this.options.testnet || false,
          this.options.subkeyAddress,
          this.options,
        );
        this.log(`JWT manager initialized with subkey: ${this.options.subkeyAddress}`, LogSeverity.Debug);
      } else {
        throw new Error('Paradex SDK initialization returned null account');
      }
    } catch (error) {
      this.log(`Failed to initialize Paradex SDK: ${error}`, LogSeverity.Error);
      // Re-throw the error so it's visible and stops exchange initialization
      throw error;
    }
  }

  private resetSDKStateAfterInitFailure() {
    this.sdkInitPromise = null;
    this.sdkAccount = null;
    this.requestSigner = null;
    this.jwtManager = null;
  }

  private async ensurePrivateHistoryAuth(operation: string) {
    let lastError: unknown;

    for (let attempt = 0; attempt <= PRIVATE_HISTORY_AUTH_RETRY_DELAYS_MS.length; attempt += 1) {
      try {
        if (this.jwtManager) {
          return;
        }

        if (!this.sdkInitPromise && this.options.paradexAddress) {
          this.log(`Retrying Paradex SDK initialization for ${operation}`, LogSeverity.Debug);
          this.sdkInitPromise = this.initializeSDK();
          void this.sdkInitPromise.catch(() => {});
        }

        if (this.sdkInitPromise) {
          await this.sdkInitPromise;
        }

        if (this.jwtManager) {
          return;
        }

        lastError = new Error('JWT manager not initialized');
        this.resetSDKStateAfterInitFailure();
      } catch (err) {
        lastError = err;
        this.resetSDKStateAfterInitFailure();
      }

      const delay = PRIVATE_HISTORY_AUTH_RETRY_DELAYS_MS[attempt];
      if (delay !== undefined) {
        await wait(delay);
      }
    }

    throw new ParadexPrivateHistoryAuthNotReadyError(operation, lastError);
  }

  dispose = () => {
    this.isDisposed = true;
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  async getPositionMode(): Promise<PositionMode> {
    // Paradex uses one-way mode
    return PositionMode.OneWay;
  }

  getAccount = async () => {
    try {
      const { data } = await this.xhr.get<ACCOUNT_RESPONSE>(ENDPOINTS.ACCOUNT);
      return {
        userId: data.account_number,
        affiliateId: null,
      };
    } catch (err) {
      this.log(`Error fetching account: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  validateAccount = async () => {
    try {
      // Initialize SDK if not already started
      // This handles the case where validateAccount is called before start()
      if (!this.sdkInitPromise && this.options.paradexAddress) {
        this.log('SDK not initialized yet, starting initialization...', LogSeverity.Debug);
        this.sdkInitPromise = this.initializeSDK();
      }

      // Wait for SDK initialization before making authenticated requests
      if (this.sdkInitPromise) {
        await this.sdkInitPromise;
      }

      // Check if JWT manager is available (required for authentication)
      if (!this.jwtManager) {
        // Provide specific error messages based on what's missing
        if (!this.options.paradexAddress) {
          return { error: 'Missing Paradex address - please enter your Paradex account address' };
        }
        if (!this.options.subkeyAddress) {
          return { error: 'Missing subkey address - please enter your subkey public address' };
        }
        if (!this.options.key) {
          return { error: 'Missing subkey private key - please enter your subkey private key' };
        }
        return { error: 'Authentication failed - SDK initialization error' };
      }

      await this.xhr.get(ENDPOINTS.ACCOUNT);
      return { error: '' };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.error?.message || err?.message);
      return { error: err?.response?.data?.error?.message || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
    try {
      this.log('Starting Paradex exchange...', LogSeverity.Info);

      const useExternalPublicData = await this.initExternalPublicData();
      const publicDataOnly = this.isPublicDataOnly;

      // Wait for SDK initialization if it's in progress (skip for public-only mode)
      if (!publicDataOnly && this.sdkInitPromise) {
        this.log('Waiting for SDK initialization...', LogSeverity.Debug);
        await this.sdkInitPromise;
        this.log('SDK initialization complete', LogSeverity.Debug);
      }

      // Check if SDK is properly initialized for authenticated requests
      if (!publicDataOnly && !this.requestSigner) {
        this.log('WARNING: Request signer not available - authentication may fail', LogSeverity.Warning);
      }

      if (!useExternalPublicData) {
        this.log('Fetching markets...', LogSeverity.Debug);
        const markets = await this.fetchMarkets();
        if (this.isDisposed) return;
        this.log(`Fetched ${markets.length} markets`, LogSeverity.Debug);

        this.store.update({
          markets,
          loaded: { ...this.store.loaded, markets: true },
        });

        this.log('Fetching tickers...', LogSeverity.Debug);
        const tickers = await this.fetchTickers();
        if (this.isDisposed) return;
        this.log(`Fetched ${tickers.length} tickers`, LogSeverity.Debug);

        this.log(`Loaded ${Math.min(tickers.length, markets.length)} Paradex markets`, LogSeverity.Info);

        this.store.update({
          tickers,
          loaded: { ...this.store.loaded, tickers: true },
        });

        this.log('Connecting to public WebSocket...', LogSeverity.Debug);
        this.publicWebsocket.connectAndSubscribe();
      }

      if (publicDataOnly) {
        this.store.update({
          loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
        });
        this.log('Paradex exchange started in public-only mode', LogSeverity.Info);
        return;
      }

      // Only connect private WebSocket if JWT manager is available
      if (this.jwtManager) {
        this.log('Connecting to private WebSocket...', LogSeverity.Debug);
        this.privateWebsocket.connectAndSubscribe();
      } else {
        this.log('Skipping private WebSocket connection - no authentication available', LogSeverity.Debug);
      }

      // Keep private account sync running even if ticker cache is temporarily empty
      // (e.g., external public data mode or delayed public bootstrap).
      if (this.store.tickers.length === 0) {
        this.log('No tickers loaded yet - continuing with private account initialization', LogSeverity.Warning);
      }

      this.log('Fetching initial leverage settings...', LogSeverity.Debug);
      await this.fetchLeverageSettings();
      if (this.isDisposed) return;

      this.log('Running initial tick (fetching positions and balance)...', LogSeverity.Debug);
      await this.tick();
      if (this.isDisposed) return;

      this.log(`Ready to trade on Paradex`, LogSeverity.Info);

      this.log('Fetching orders...', LogSeverity.Debug);
      const orders = await this.fetchOrders();
      if (this.isDisposed) return;
      this.log(`Fetched ${orders.length} orders`, LogSeverity.Debug);

      this.log(`Loaded Paradex orders`, LogSeverity.Info);

      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });

      this.log('Paradex exchange started successfully', LogSeverity.Info);
    } catch (error) {
      this.log(`Error starting Paradex exchange: ${error}`, LogSeverity.Error);
      throw error;
    }
  };

  tick = async () => {
    if (!this.isDisposed) {
      try {
        this.log(`Tick: Fetching positions...`, LogSeverity.Debug);
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.log(`Tick: Fetched ${positions.length} positions`, LogSeverity.Debug);
        this.store.update({ positions, loaded: { ...this.store.loaded, positions: true } });

        this.log(`Tick: Fetching balance...`, LogSeverity.Debug);
        const balance = await this.fetchBalance();
        if (this.isDisposed) return;
        this.log(`Tick: Balance fetched - total: ${balance.total}, free: ${balance.free}`, LogSeverity.Debug);

        const upnl = positions.length > 0 ? sumBy(positions, 'unrealizedPnl') : 0;
        (balance as Writable<Balance>).upnl = upnl;

        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });

        // Fetch orders periodically (every 4th tick = every 40 seconds with 10s interval)
        // Note: This needs to be tracked differently without count parameter
        this.log(`Tick: Fetching orders (periodic refresh)...`, LogSeverity.Debug);
        const orders = await this.fetchOrders();
        if (this.isDisposed) return;
        this.log(`Tick: Fetched ${orders.length} orders`, LogSeverity.Debug);
        this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });

        // Fetch leverage settings to refresh our local cache
        this.log(`Tick: Refreshing leverage settings...`, LogSeverity.Debug);
        await this.fetchLeverageSettings();
        if (this.isDisposed) return;
      } catch (err: any) {
        this.log(`Error in tick: ${err?.message}`, LogSeverity.Error);
        this.log(`Tick error stack: ${err?.stack}`, LogSeverity.Debug);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), 10_000);
      }
    }
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      // Ensure SDK is initialized before making authenticated requests
      if (this.sdkInitPromise) {
        await this.sdkInitPromise;
      }

      // Verify we have authentication set up
      if (!this.jwtManager) {
        this.log('Cannot fetch balance: JWT manager not initialized', LogSeverity.Warning);
        return { total: 0, free: 0, used: 0, upnl: 0 };
      }

      this.log('Fetching account balance from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<ACCOUNT_RESPONSE>(ENDPOINTS.ACCOUNT);

      const account = ((data as any)?.data ?? data) as Record<string, unknown> | undefined;
      if (!account) {
        this.log('Balance API returned invalid data', LogSeverity.Error);
        throw new Error('Balance API returned no data');
      }

      const readNumber = (...values: unknown[]): number | undefined => {
        for (const value of values) {
          if (value === undefined || value === null) {
            continue;
          }
          const parsed = Number.parseFloat(String(value));
          if (Number.isFinite(parsed)) {
            return parsed;
          }
        }
        return undefined;
      };

      const total =
        readNumber(
          account.total_collateral,
          account.account_value,
          account.equity,
          account.margin_balance,
          account.wallet_balance,
        ) ?? 0;
      const free =
        readNumber(account.free_collateral, account.free, account.available_margin, account.available_collateral) ?? 0;
      const used =
        readNumber(account.initial_margin_requirement, account.position_margin, account.used_margin) ??
        Math.max(total - free, 0);

      this.log(
        `Balance API response: total_collateral=${account.total_collateral}, account_value=${account.account_value}, equity=${account.equity}, free_collateral=${account.free_collateral}, position_margin=${account.position_margin}, initial_margin_requirement=${account.initial_margin_requirement}`,
        LogSeverity.Debug,
      );

      // Paradex has shipped multiple account payload shapes across endpoints/streams.
      // Prefer collateral-style fields, but gracefully fallback to equity/position margin variants.
      return {
        total,
        free,
        used,
        upnl: 0, // Will be calculated from positions
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      this.log(`Balance error details: ${describeError(err)}`, LogSeverity.Debug);
      throw err;
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      this.log('Fetching markets from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<{ results: MARKET_RESPONSE[] }>(ENDPOINTS.MARKETS);

      if (!data || !data.results) {
        this.log('Markets API returned invalid data structure', LogSeverity.Error);
        this.log(`Data received: ${JSON.stringify(data)}`, LogSeverity.Debug);
        return [];
      }

      this.log(`Markets API response: ${data.results.length} total markets`, LogSeverity.Debug);

      // Log first market to see available fields
      if (data.results.length > 0) {
        this.log(`Sample market fields: ${Object.keys(data.results[0]).join(', ')}`, LogSeverity.Debug);
        this.log(`Sample market data: ${JSON.stringify(data.results[0])}`, LogSeverity.Debug);
      }

      // Log status values to debug filtering
      const statusCounts = data.results.reduce(
        (acc, m) => {
          acc[m.status || 'undefined'] = (acc[m.status || 'undefined'] || 0) + 1;
          return acc;
        },
        {} as Record<string, number>,
      );
      this.log(`Market status distribution: ${JSON.stringify(statusCounts)}`, LogSeverity.Debug);

      // Log asset_kind distribution
      const assetKindCounts = data.results.reduce(
        (acc, m) => {
          acc[m.asset_kind || 'undefined'] = (acc[m.asset_kind || 'undefined'] || 0) + 1;
          return acc;
        },
        {} as Record<string, number>,
      );
      this.log(`Asset kind distribution: ${JSON.stringify(assetKindCounts)}`, LogSeverity.Debug);

      // Filter for active perpetual markets. Paradex returns options and spot
      // markets too, and subscribing public streams for all of them exceeds
      // the exchange's per-connection subscription limit.
      const activeMarkets = data.results.filter((m) => {
        const symbol = m.market || (m as any).symbol;
        const assetKind = String(m.asset_kind || '').toUpperCase();
        const marketKind = String((m as any).market_kind || '').toUpperCase();
        const isOption =
          assetKind.includes('OPTION') ||
          marketKind.includes('OPTION') ||
          symbol?.endsWith('-C') ||
          symbol?.endsWith('-P');
        const isPerpetual = !assetKind || assetKind === 'PERP' || symbol?.endsWith('-PERP');
        const isActive = !m.status || m.status === 'ACTIVE';
        return symbol && isActive && isPerpetual && !isOption;
      });
      this.log(`Active perpetual futures markets (excluding options): ${activeMarkets.length}`, LogSeverity.Debug);

      return activeMarkets.map((m) => {
        const symbol = m.market || (m as any).symbol;

        // Calculate max leverage from Initial Margin Fraction (IMF)
        // Max leverage = 1 / IMF (e.g., IMF of 0.02 = 50x leverage)
        let maxLeverage = 100; // Default fallback
        if (m.delta1_cross_margin_params?.imf_base) {
          const imf = parseFloat(m.delta1_cross_margin_params.imf_base);
          if (imf > 0) {
            maxLeverage = Math.floor(1 / imf);
          }
        } else if (m.initial_margin_fraction) {
          const imf = parseFloat(m.initial_margin_fraction);
          if (imf > 0) {
            maxLeverage = Math.floor(1 / imf);
          }
        }

        return {
          id: symbol,
          symbol: symbol,
          base: m.base_currency,
          quote: m.quote_currency,
          active: !m.status || m.status === 'ACTIVE', // Consistent with filter logic
          type: 'swap',
          spot: false,
          swap: true,
          future: false,
          contract: true,
          linear: true,
          inverse: false,
          contractSize: 1,
          precision: {
            amount: parseFloat(m.step_size || (m as any).order_size_increment),
            price: parseFloat(m.tick_size || (m as any).price_tick_size),
            base: 8,
            quote: 8,
          },
          limits: {
            amount: {
              min: parseFloat(m.min_order_size || (m as any).min_notional || '0'),
              max: parseFloat(m.max_order_size || (m as any).max_order_size || '0'),
            },
            price: { min: 0, max: 0 },
            cost: { min: 0, max: 0 },
            leverage: { min: 1, max: maxLeverage },
          },
          fees: {
            maker: parseFloat(m.maker_fee || '0'),
            taker: parseFloat(m.taker_fee || '0'),
          },
          info: m,
        };
      });
    } catch (err) {
      this.log(`Error fetching markets: ${err}`, LogSeverity.Error);
      this.log(`Markets error details: ${describeError(err)}`, LogSeverity.Debug);
      throw err;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      this.log('Fetching tickers from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<{ results: TICKER_RESPONSE[] }>(`${ENDPOINTS.TICKERS}?market=ALL`);

      if (!data || !data.results) {
        this.log('Tickers API returned invalid data structure', LogSeverity.Error);
        this.log(`Data received: ${JSON.stringify(data)}`, LogSeverity.Debug);
        return [];
      }

      this.log(`Tickers API response: ${data.results.length} total tickers`, LogSeverity.Debug);

      // Keep perpetual futures only. Spot/options are not represented as swaps in this integration.
      const perpTickers = data.results.filter((t) => {
        const isOption = t.symbol.endsWith('-C') || t.symbol.endsWith('-P');
        return t.symbol.endsWith('-PERP') && !isOption;
      });
      this.log(`Perpetual futures tickers (excluding options): ${perpTickers.length}`, LogSeverity.Debug);

      return perpTickers.map((t) => ({
        id: t.symbol,
        symbol: t.symbol,
        bid: parseFloat(t.bid || '0'),
        ask: parseFloat(t.ask || '0'),
        last: parseFloat(t.last_traded_price || '0'),
        mark: parseFloat(t.mark_price || '0'),
        index: parseFloat(t.underlying_price || t.mark_price || '0'),
        percentage: parseFloat(t.price_change_rate_24h || '0') * 100,
        fundingRate: parseFloat(t.funding_rate || '0'),
        fundingTime: 0, // Not provided in summary endpoint
        volume: parseFloat(t.volume_24h || '0'),
        quoteVolume: parseFloat(t.volume_24h || '0'), // Paradex only provides 24h volume
        openInterest: parseFloat(t.open_interest || '0'),
        timestamp: virtualClock.getCurrentTimeMs(),
        info: t,
      }));
    } catch (err) {
      this.log(`Error fetching tickers: ${err}`, LogSeverity.Error);
      this.log(`Tickers error details: ${describeError(err)}`, LogSeverity.Debug);
      throw err;
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      const klineResolution: Partial<Record<OHLCVOptions['interval'], string>> = {
        '1m': '1',
        '3m': '3',
        '5m': '5',
        '15m': '15',
        '30m': '30',
        '1h': '60',
      };
      const documentedResolution = klineResolution[opts.interval];

      // Prefer the published /markets/klines contract for every interval it supports.
      // Keep TradingView only for wider intervals and as a temporary compatibility
      // fallback while Paradex still serves that undocumented route.
      if (documentedResolution) {
        const endAt = opts.to ?? virtualClock.getCurrentTimeMs();
        const intervalMs = timeframeToSeconds[opts.interval] * 1000;
        const startAt = opts.from ?? endAt - intervalMs * 2000;

        try {
          const { data } = await this.xhr.get(ENDPOINTS.OHLCV, {
            params: {
              symbol: opts.symbol,
              resolution: documentedResolution,
              start_at: startAt,
              end_at: endAt,
              price_kind: 'last',
            },
          });
          if (Array.isArray(data?.results)) {
            return data.results.flatMap((row: unknown) => {
              if (!Array.isArray(row) || row.length < 6) return [];
              const [timestamp, open, high, low, close, volume] = row.map(Number);
              if (![timestamp, open, high, low, close, volume].every(Number.isFinite)) return [];
              return [{ timestamp, open, high, low, close, volume }];
            });
          }
        } catch (err: any) {
          this.log(
            `Documented OHLCV endpoint failed for ${opts.symbol}; trying legacy TradingView: ${err?.message || err}`,
            LogSeverity.Warning,
          );
        }
      }

      // Paradex TradingView endpoint: /v1/tradingview/history
      // Supports more intervals than /markets/klines including 1D, 4H, etc.
      // Parameters:
      // - symbol: market symbol
      // - resolution: TradingView format (1, 5, 15, 60, 240, 1D, 1W, etc.)
      // - from: Unix timestamp in SECONDS
      // - to: Unix timestamp in SECONDS
      // - price_kind: 'last', 'mark', or 'index'

      // Convert interval string to TradingView resolution format
      const resolution = this.convertIntervalToTradingViewResolution(opts.interval);

      const params = omitUndefined({
        symbol: opts.symbol,
        resolution: resolution,
        from: opts.from ? Math.floor(opts.from / 1000) : undefined, // Convert ms to seconds
        to: opts.to ? Math.floor(opts.to / 1000) : undefined, // Convert ms to seconds
        countback: 2000, // Maximum number of candles to return
        price_kind: 'last', // Use last traded price
      });

      const endpoint = ENDPOINTS.TRADINGVIEW_HISTORY;

      const { data } = await this.xhr.get(endpoint, { params });

      // TradingView format: {s: 'ok', t: [], o: [], h: [], l: [], c: [], v: []}
      if (!data || data.s !== 'ok') {
        this.log(`OHLCV API returned error status: ${data?.s || 'unknown'}`, LogSeverity.Error);
        return [];
      }

      if (!data.t || !Array.isArray(data.t)) {
        this.log('OHLCV API returned invalid data structure', LogSeverity.Error);
        return [];
      }

      // Convert TradingView format to our Candle format
      const candles: Candle[] = [];
      for (let i = 0; i < data.t.length; i++) {
        candles.push({
          timestamp: data.t[i] * 1000, // Convert seconds to milliseconds
          open: parseFloat(data.o[i]),
          high: parseFloat(data.h[i]),
          low: parseFloat(data.l[i]),
          close: parseFloat(data.c[i]),
          volume: parseFloat(data.v[i]),
        });
      }

      return candles;
    } catch (err: any) {
      this.log(`Error fetching OHLCV for ${opts.symbol}: ${err.message || err}`, LogSeverity.Error);
      throw err;
    }
  };

  /**
   * Convert interval string (e.g., "1m", "5m", "1h", "1d") to TradingView resolution format
   * TradingView uses: minutes as numbers (1, 5, 15, 60, 240), days as '1D', weeks as '1W'
   */
  private convertIntervalToTradingViewResolution(interval: string): string {
    const intervalMatch = interval.match(/^(\d+)([mhdwMy])$/);
    if (!intervalMatch) {
      throw new Error(
        `Invalid interval format: ${interval}. Expected format: <number><unit> (e.g., "1m", "5m", "1h", "1d")`,
      );
    }

    const value = parseInt(intervalMatch[1]);
    const unit = intervalMatch[2];

    switch (unit) {
      case 'm':
        return value.toString(); // Minutes: '1', '5', '15', etc.
      case 'h':
        return (value * 60).toString(); // Hours to minutes: '60', '240', etc.
      case 'd':
        return `${value}D`; // Days: '1D', '2D', etc.
      case 'w':
      case 'W':
        return `${value}W`; // Weeks: '1W', '2W', etc.
      case 'M':
        return `${value}M`; // Months: '1M', '3M', etc.
      case 'y':
        return `${value * 12}M`; // Years to months: '12M', '24M', etc.
      default:
        throw new Error(`Unsupported interval unit: ${unit}`);
    }
  }

  fetchOrders = async (): Promise<Order[]> => {
    try {
      // Ensure SDK is initialized before making authenticated requests
      if (this.sdkInitPromise) {
        await this.sdkInitPromise;
      }

      // Verify we have authentication set up
      if (!this.jwtManager) {
        this.log('Cannot fetch orders: JWT manager not initialized', LogSeverity.Warning);
        return [];
      }

      this.log('Fetching orders from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<{ results: ORDER_RESPONSE[] }>(ENDPOINTS.ORDERS);

      if (!data || !data.results) {
        this.log('Orders API returned invalid data structure', LogSeverity.Error);
        this.log(`Data received: ${JSON.stringify(data)}`, LogSeverity.Debug);
        return [];
      }

      this.log(`Orders API response: ${data.results.length} total orders`, LogSeverity.Debug);

      const openOrders = data.results.filter((o) => ['NEW', 'OPEN', 'PENDING', 'UNTRIGGERED'].includes(o.status));
      this.log(`Open orders: ${openOrders.length}`, LogSeverity.Debug);

      return openOrders.map(this.mapOrder);
    } catch (err) {
      this.log(`Error fetching orders: ${err}`, LogSeverity.Error);
      this.log(`Orders error details: ${describeError(err)}`, LogSeverity.Debug);
      throw err;
    }
  };

  /**
   * Account ledger. Every term rides on the fill — `realized_pnl`, `fee` and
   * `realized_funding` — so unlike every other venue here this needs no second endpoint.
   * Field names verified live; see `paradex.income.ts`.
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    if (this.sdkInitPromise) {
      await this.sdkInitPromise;
    }
    if (!this.jwtManager) {
      this.log('Skipping fetchIncomeHistory: JWT manager not initialized', LogSeverity.Warning);
      return [];
    }

    const endTime = opts?.endTime ?? Date.now();
    const startTime = opts?.startTime ?? endTime - 30 * 24 * 60 * 60 * 1000;
    const market = opts?.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts?.symbol && !market) {
      return [];
    }

    const resolveSymbol = (id: string) => this.store.marketsById[id]?.symbol;
    const MAX_PAGES = 200;
    const PAGE_SIZE = 500;

    try {
      const records: IncomeRecord[] = [];
      const seen = new Set<string>();
      let cursor: string | undefined;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        const params = omitUndefined({
          cursor,
          market: market?.symbol,
          page_size: PAGE_SIZE,
          start_at: startTime,
          end_at: endTime,
        });
        const response = await this.xhr.get(ENDPOINTS.FILLS, { params });
        const body = response?.data;
        const rows: Array<Record<string, any>> = Array.isArray(body?.results)
          ? body.results
          : Array.isArray(body)
            ? body
            : [];

        for (const row of rows) {
          const id = String(row.id ?? '');
          if (id && seen.has(id)) continue;
          if (id) seen.add(id);
          records.push(...mapParadexFillToIncomeRecords(row, resolveSymbol));
        }

        const next = typeof body?.next === 'string' && body.next ? body.next : undefined;
        if (!next || next === cursor) break;
        cursor = next;
      }

      if (page >= MAX_PAGES) {
        // Fills come newest-first, so a cap that bites drops the OLDEST rows of the window
        // — a hole mid-ledger rather than a short tail.
        this.log(
          `fetchIncomeHistory stopped at the ${MAX_PAGES}-page cap; older fills in this window were not fetched`,
          LogSeverity.Warning,
        );
      }

      return records.sort((a, b) => b.timestamp - a.timestamp);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    // No chunking: paradex bounds a response by page size and walks with a cursor.
    const records = await this.fetchIncomeHistory({ startTime, endTime, incomeType });
    return incomeType ? records.filter((record) => record.incomeType === incomeType) : records;
  };

  /**
   * Historical orders, including ones that never filled — see `paradex.order-history.ts`.
   *
   * `start_at` / `end_at` ARE honoured here (a window the account has no orders in comes
   * back `results: []`), which is worth stating because four other venues in this epic
   * answered a windowed request with their full history and no error.
   */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const MAX_PAGES = 50;
    const PAGE_SIZE = 100;

    try {
      await this.ensurePrivateHistoryAuth('fetchHistoricalOrders');

      const orders: HistoricalOrder[] = [];
      const seen = new Set<string>();
      let cursor: string | undefined;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        const { data } = await this.xhr.get(ENDPOINTS.ORDERS_HISTORY, {
          params: omitUndefined({
            market: opts?.symbol,
            start_at: opts?.from,
            end_at: opts?.to,
            page_size: PAGE_SIZE,
            cursor,
          }),
        });

        const rows: Array<Record<string, any>> = Array.isArray(data?.results) ? data.results : [];

        for (const row of rows) {
          const order = mapParadexHistoricalOrder(row);
          if (!order || seen.has(order.id)) continue;
          seen.add(order.id);
          orders.push(order);
        }

        cursor = typeof data?.next === 'string' && data.next ? data.next : undefined;
        if (!cursor) break;
      }

      if (page >= MAX_PAGES) {
        this.log(
          `fetchHistoricalOrders stopped at the ${MAX_PAGES}-page cap; older orders were not fetched`,
          LogSeverity.Warning,
        );
      }

      const sorted = orders.sort((a, b) => b.createdTime - a.createdTime);
      return opts?.limit && opts.limit > 0 ? sorted.slice(0, opts.limit) : sorted;
    } catch (err: any) {
      if (isParadexPrivateHistoryAuthNotReadyError(err)) {
        this.log(err.message, LogSeverity.Warning);
        throw err;
      }
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 100 } = opts;
    if (!symbol) {
      return [];
    }

    const market = this.store.marketsBySymbol[symbol];
    if (!market) {
      return [];
    }

    const maxLimit = Math.min(Math.max(limit, 1), 500);

    const normalizeToMilliseconds = (value?: number) => {
      if (value === undefined || value === null || !Number.isFinite(value)) {
        return undefined;
      }
      return value > 1_000_000_000_000 ? Math.floor(value) : Math.floor(value * 1000);
    };

    const normalizeToSeconds = (value: unknown) => {
      const parsed = Number.parseInt(String(value ?? '0'), 10);
      if (!Number.isFinite(parsed) || parsed <= 0) {
        return undefined;
      }
      return parsed > 1_000_000_000_000 ? Math.floor(parsed / 1000) : parsed;
    };

    const fromMilliseconds = normalizeToMilliseconds(from);
    const toMilliseconds = normalizeToMilliseconds(to);

    const parseFillRows = (rows: Array<Record<string, any>>, aggregateOrders = false) =>
      rows.reduce<Execution[]>((acc, row) => {
        const rowSymbol = String(row.market || row.symbol || '');
        if (rowSymbol !== market.symbol) {
          return acc;
        }

        const side = ORDER_SIDE[String(row.side || '').toUpperCase()];
        if (!side) {
          return acc;
        }

        const price = parseFloat(row.price || row.avg_fill_price || row.average_fill_price || '0');
        const filledFromOrder =
          parseFloat(row.size || '0') - parseFloat(row.remaining_size || row.remainingSize || '0');
        const qty = parseFloat(
          row.fill_size || row.filled_size || row.quantity || (aggregateOrders ? (filledFromOrder > 0 ? filledFromOrder : 0) : row.size || '0'),
        );
        if (!Number.isFinite(price) || !Number.isFinite(qty) || price <= 0 || qty <= 0) {
          return acc;
        }

        const timestamp = normalizeToSeconds(row.created_at || row.timestamp || row.updated_at);
        if (!timestamp) {
          return acc;
        }

        const executionOrderId = String(row.order_id || row.orderId || (aggregateOrders ? row.id : undefined) || row.client_id || '');
        if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
          return acc;
        }

        const fee = parseFloat(row.fee || '0');
        const realizedPnl = parseFloat(row.realized_pnl ?? '');
        acc.push(
          omitUndefined({
            // Paradex gives every fill a unique `id`. Without it, two genuine fills of one
            // order at the same timestamp, price and size collapse into one — the exact
            // data loss the field exists to prevent.
            id: row.id ? String(row.id) : undefined,
            timestamp,
            symbol: rowSymbol,
            orderId: executionOrderId || undefined,
            price,
            qty,
            side,
            commission: Number.isFinite(fee) ? Math.abs(fee) : undefined,
            commissionAsset: row.fee_currency || market.quote,
            // Paradex fills carry no position side — `side` is the order side —
            // so positionSide is left absent rather than inferred.
            realizedPnl: Number.isFinite(realizedPnl) ? realizedPnl : undefined,
            maker: row.liquidity ? String(row.liquidity).toUpperCase() === 'MAKER' : undefined,
          }),
        );
        return acc;
      }, []);

    const extractRows = (payload: any): Array<Record<string, any>> => {
      const body = payload?.data ?? payload;
      const candidates = [body?.results, body?.fills, body?.data, body];
      for (const candidate of candidates) {
        if (Array.isArray(candidate)) {
          return candidate;
        }
      }
      return [];
    };

    const fetchFillRows = async () => {
      const rows: Array<Record<string, any>> = [];
      let cursor: string | undefined;
      let page = 0;
      const maxPages = 5;

      while (page < maxPages && rows.length < maxLimit) {
        const params = omitUndefined({
          cursor,
          market: market.symbol,
          page_size: maxLimit,
          end_at: toMilliseconds,
        });
        const response = await this.xhr.get(ENDPOINTS.FILLS, { params });
        const body = response?.data;
        rows.push(...extractRows(body));
        cursor = typeof body?.next === 'string' && body.next ? body.next : undefined;
        if (!cursor) {
          break;
        }
        page += 1;
      }
      return rows;
    };

    const fetchOrderFallbackRows = async () => {
      const response = await this.xhr.get(ENDPOINTS.ORDERS_HISTORY, {
        params: omitUndefined({
          market: market.symbol,
          limit: maxLimit,
        }),
      });
      const rows = extractRows(response?.data);
      return rows.filter((row) => {
        const filledSize = parseFloat(row.size || '0') - parseFloat(row.remaining_size || row.remainingSize || '0');
        return filledSize > 0;
      });
    };

    try {
      await this.ensurePrivateHistoryAuth('fetchHistoricalFills');

      const primaryRows = await fetchFillRows();
      const fallbackRows = primaryRows.length > 0 ? [] : await fetchOrderFallbackRows();
      const deduped = new Map<string, Execution>();

      for (const execution of [...parseFillRows(primaryRows), ...parseFillRows(fallbackRows, true)]) {
        if (fromMilliseconds && execution.timestamp * 1000 < fromMilliseconds) {
          continue;
        }
        if (toMilliseconds && execution.timestamp * 1000 > toMilliseconds) {
          continue;
        }
        const key = execution.id ?? `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
        deduped.set(key, execution);
      }

      return [...deduped.values()].sort((a, b) => b.timestamp - a.timestamp).slice(0, maxLimit);
    } catch (err: any) {
      if (isParadexPrivateHistoryAuthNotReadyError(err)) {
        this.log(err.message, LogSeverity.Warning);
        throw err;
      }
      this.log(`Error fetching executions: ${err?.message || err}`, LogSeverity.Error);
      return [];
    }
  };

  fetchPositions = async (): Promise<Position[]> => {
    try {
      // Ensure SDK is initialized before making authenticated requests
      if (this.sdkInitPromise) {
        await this.sdkInitPromise;
      }

      // Verify we have authentication set up
      if (!this.jwtManager) {
        this.log('Cannot fetch positions: JWT manager not initialized', LogSeverity.Warning);
        return [];
      }

      this.log('Fetching positions from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<{ results: POSITION_RESPONSE[] }>(ENDPOINTS.POSITIONS);

      if (!data || !data.results) {
        this.log('Positions API returned invalid data structure', LogSeverity.Error);
        this.log(`Data received: ${JSON.stringify(data)}`, LogSeverity.Debug);
        return [];
      }

      this.log(`Positions API response: ${data.results.length} total positions`, LogSeverity.Debug);

      const openPositions = data.results.filter((p) => p.status === 'OPEN');
      this.log(`Open positions: ${openPositions.length}`, LogSeverity.Debug);

      // Transform actual positions from API
      const actualPositions: Position[] = openPositions.map((p) => {
        const entryPrice = parseFloat(p.average_entry_price);
        const contracts = Math.abs(parseFloat(p.size));
        const apiLeverage = parseFloat(p.leverage || '0');
        // Use API leverage if available, otherwise use stored leverage, fallback to 10
        const leverage = apiLeverage > 0 ? apiLeverage : this.leverageSettings.get(p.market) || 10;

        return {
          id: createPositionId(
            p.market,
            p.side === 'LONG' ? PositionSide.Long : PositionSide.Short,
            PositionMode.OneWay,
          ),
          symbol: p.market,
          side: p.side === 'LONG' ? PositionSide.Long : PositionSide.Short,
          contracts,
          unrealizedPnl: parseFloat(p.unrealized_pnl),
          realizedPnl: parseFloat(p.realized_positional_pnl || '0'),
          notional: contracts * entryPrice,
          leverage,
          entryPrice,
          liquidationPrice: parseFloat(p.liquidation_price || '0'),
          marginType: MarginType.Cross, // Paradex uses cross margin
          positionMode: PositionMode.OneWay, // Paradex only supports one-way mode
          lastUpdate: p.last_updated_at,
          info: p,
        };
      });

      // Create fake positions (0 contracts) for ALL active markets
      // This allows the UI to show all available symbols
      // For OneWay mode, we only need one position per symbol (system handles netting)

      // Check if markets are loaded before creating fake positions
      if (!this.store.markets || this.store.markets.length === 0) {
        this.log(
          'WARNING: Markets not loaded yet - cannot create fake positions. Returning only actual positions.',
          LogSeverity.Warning,
        );
        this.log(
          'This usually happens if fetchPositions is called before markets are loaded during exchange startup.',
          LogSeverity.Debug,
        );
        return actualPositions;
      }

      // Debug: Check how many markets are marked as active
      const activeMarketsCount = this.store.markets.filter((m) => m.active).length;
      this.log(`Markets: total=${this.store.markets.length}, active=${activeMarketsCount}`, LogSeverity.Debug);

      const fakePositions: Position[] = this.store.markets
        .filter((market) => market.active)
        .map((market) => ({
          id: createPositionId(market.symbol, PositionSide.Long, PositionMode.OneWay),
          symbol: market.symbol,
          side: PositionSide.Long, // Default to Long for display
          entryPrice: 0,
          notional: 0,
          leverage: this.leverageSettings.get(market.symbol) || 10, // Use stored leverage or default to 10
          unrealizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
          marginType: MarginType.Cross, // Paradex default
          positionMode: PositionMode.OneWay, // Paradex uses one-way mode
          info: {},
        }));

      this.log(
        `Created ${fakePositions.length} fake positions for all ${this.store.markets.length} active markets`,
        LogSeverity.Debug,
      );

      // Merge actual positions with fake positions
      // Actual positions will overwrite fake ones for the same symbol
      const positionMap = new Map<string, Position>();

      // First add all fake positions
      for (const pos of fakePositions) {
        positionMap.set(pos.symbol, pos);
      }

      // Then overwrite with actual positions (they take precedence)
      for (const pos of actualPositions) {
        positionMap.set(pos.symbol, pos);
      }

      return Array.from(positionMap.values());
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      this.log(`Positions error details: ${describeError(err)}`, LogSeverity.Debug);
      throw err;
    }
  };

  fetchPublicTrades = async (symbol: string, limit = 100): Promise<PublicTrade[]> => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) return [];
    const { data } = await this.xhr.get('/trades', { params: { market: market.id, page_size: Math.min(Math.max(limit, 1), 500) } });
    if (!Array.isArray(data?.results)) throw new Error('Invalid Paradex trades envelope');
    const trades: PublicTrade[] = [];
    const seen = new Set<string>();
    for (const row of data.results) {
      const price = Number(row.price);
      const qty = Number(row.size);
      const timestamp = Number(row.created_at);
      const side = ORDER_SIDE[row.side];
      if (typeof row.id !== 'string' || !row.id || !side || row.market !== market.id ||
          !Number.isFinite(price) || price <= 0 || !Number.isFinite(qty) || qty <= 0 ||
          !Number.isFinite(timestamp) || timestamp <= 0) throw new Error('Invalid Paradex public trade');
      if (seen.has(row.id)) continue;
      seen.add(row.id);
      trades.push({ id: row.id, symbol, side, price, qty, quoteQty: price * qty, timestamp });
    }
    return trades;
  };

  fetchOrderBook = async (symbol: string, depth = 20): Promise<OrderBook> => {
    try {
      this.log(`Fetching orderbook for ${symbol} with depth=${depth}`, LogSeverity.Debug);

      const endpoint = ENDPOINTS.ORDERBOOK.replace('{market}', symbol);
      this.log(`Orderbook endpoint: GET ${endpoint}?depth=${depth}`, LogSeverity.Debug);

      const startTime = Date.now();
      const { data } = await this.xhr.get<ORDERBOOK_RESPONSE>(endpoint, { params: { depth } });
      const duration = Date.now() - startTime;

      if (!data || !data.bids || !data.asks) {
        this.log(`Orderbook API returned invalid data structure for ${symbol}`, LogSeverity.Error);
        this.log(`Data received: ${JSON.stringify(data)}`, LogSeverity.Debug);
        throw new Error('Invalid orderbook data structure');
      }

      const orderbook = {
        bids: data.bids.map(([price, size]) => ({
          price: parseFloat(price),
          amount: parseFloat(size),
          total: parseFloat(price) * parseFloat(size),
        })),
        asks: data.asks.map(([price, size]) => ({
          price: parseFloat(price),
          amount: parseFloat(size),
          total: parseFloat(price) * parseFloat(size),
        })),
      };

      this.log(
        `Fetched orderbook for ${symbol} in ${duration}ms: ${orderbook.bids.length} bids, ${orderbook.asks.length} asks`,
        LogSeverity.Info,
      );

      if (orderbook.bids.length > 0 && orderbook.asks.length > 0) {
        const bestBid = orderbook.bids[0];
        const bestAsk = orderbook.asks[0];
        const spread = bestAsk.price - bestBid.price;
        const spreadPercent = (spread / bestBid.price) * 100;

        this.log(
          `Orderbook ${symbol}: Best Bid=${bestBid.price} (${bestBid.amount}), Best Ask=${bestAsk.price} (${bestAsk.amount}), Spread=${spread.toFixed(2)} (${spreadPercent.toFixed(3)}%)`,
          LogSeverity.Debug,
        );
      } else {
        this.log(`Orderbook ${symbol}: Empty orderbook received`, LogSeverity.Warning);
      }

      return orderbook;
    } catch (err: any) {
      this.log(`Error fetching orderbook for ${symbol}: ${err.message || err}`, LogSeverity.Error);
      this.log(`Orderbook error details: ${describeError(err)}`, LogSeverity.Debug);
      if (err.response) {
        this.log(`Orderbook response status: ${err.response.status}`, LogSeverity.Debug);
        this.log(`Orderbook response data: ${JSON.stringify(err.response.data)}`, LogSeverity.Debug);
      }
      throw err;
    }
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // Delegate to public WebSocket which handles OHLCV polling
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    this.resolveCallerClientOrderId(opts);
    try {
      const orderIds: string[] = [];

      // Place the main order
      const mainOrderId = await this.placeMainOrder(opts);
      orderIds.push(mainOrderId);

      // If TP or SL are provided, place conditional orders
      // These will be linked to the position created by the main order
      if (opts.takeProfit || opts.stopLoss) {
        const tpSlIds = await this.placeTakeProfitStopLoss(opts, mainOrderId);
        orderIds.push(...tpSlIds);
      }

      return orderIds;
    } catch (err: any) {
      this.log(`Error placing order: ${err}`, LogSeverity.Error);

      // Error already extracted and emitted in placeMainOrder
      // Just re-throw to propagate up the call stack
      throw err;
    }
  };

  private isNativeBatchOrder = (opts: PlaceOrderOpts): boolean => {
    const supportedTimeInForce =
      opts.timeInForce === undefined ||
      opts.timeInForce === 'GoodTillCancel' ||
      opts.timeInForce === 'ImmediateOrCancel' ||
      opts.timeInForce === 'PostOnly';

    return (
      opts.type === OrderType.Limit &&
      opts.triggerPrice === undefined &&
      opts.stopLoss === undefined &&
      opts.takeProfit === undefined &&
      opts.tpTriggerType === undefined &&
      opts.slTriggerType === undefined &&
      opts.close !== true &&
      supportedTimeInForce
    );
  };

  private generateClientId = (): string => {
    const clientId = `ts_${uuid()}`;
    if (clientId.length <= 3 || clientId.length > 64) {
      throw new Error('Paradex generated an invalid client_id');
    }
    return clientId;
  };

  private resolveMarketIocExecutionPrice = async (opts: PlaceOrderOpts): Promise<number | undefined> => {
    if (
      opts.type !== OrderType.Market ||
      typeof opts.maxSlippagePct !== 'number' ||
      !Number.isFinite(opts.maxSlippagePct)
    ) {
      return undefined;
    }
    const timeInForce = opts.timeInForce as string | undefined;
    if (timeInForce !== undefined && timeInForce !== OrderTimeInForce.ImmediateOrCancel && timeInForce !== 'IOC') {
      throw new Error('Paradex maxSlippagePct market orders require IOC time-in-force');
    }

    const orderBook = await this.fetchOrderBook(opts.symbol, 5);
    const touchPrice = opts.side === OrderSide.Buy ? orderBook.asks[0]?.price : orderBook.bids[0]?.price;
    if (!Number.isFinite(touchPrice) || !touchPrice || touchPrice <= 0) {
      throw new Error(
        `Paradex ${opts.side} market IOC order requires a live ${opts.side === OrderSide.Buy ? 'ask' : 'bid'}`,
      );
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickers.find((t) => t.symbol === opts.symbol || t.id === opts.symbol);
    const bidAskMidpoint =
      isPositiveFinitePrice(ticker?.bid) && isPositiveFinitePrice(ticker?.ask)
        ? (ticker.bid + ticker.ask) / 2
        : undefined;
    const fairPrice = [ticker?.mark, ticker?.index, ticker?.midPrice, ticker?.last, bidAskMidpoint, touchPrice].find(
      isPositiveFinitePrice,
    );
    const priceBandWidth =
      normalizePositiveFraction(market?.info?.price_bands_width) ??
      normalizePositiveFraction(market?.info?.max_slippage);
    const slippageFactor = Math.max(0, opts.maxSlippagePct) / 100;
    const priceStep = Number(market?.precision?.price || 0.000001);
    const bandLimit =
      fairPrice !== undefined && priceBandWidth !== undefined
        ? fairPrice * (opts.side === OrderSide.Buy ? 1 + priceBandWidth : 1 - priceBandWidth)
        : undefined;
    let rawPrice = (fairPrice ?? touchPrice) * (opts.side === OrderSide.Buy ? 1 + slippageFactor : 1 - slippageFactor);

    if (bandLimit !== undefined) {
      rawPrice = opts.side === OrderSide.Buy ? Math.min(rawPrice, bandLimit) : Math.max(rawPrice, bandLimit);
    }

    const touchInsideBand =
      bandLimit === undefined || (opts.side === OrderSide.Buy ? touchPrice <= bandLimit : touchPrice >= bandLimit);
    if (touchInsideBand) {
      const missesTouch = opts.side === OrderSide.Buy ? rawPrice < touchPrice : rawPrice > touchPrice;
      if (missesTouch) rawPrice = touchPrice;
    }

    let price = roundPriceToStep(rawPrice, priceStep, opts.side === OrderSide.Buy ? 'up' : 'down');
    if (bandLimit !== undefined) {
      if (opts.side === OrderSide.Buy && price > bandLimit) {
        price = roundPriceToStep(bandLimit, priceStep, 'down');
      }
      if (opts.side === OrderSide.Sell && price < bandLimit) {
        price = roundPriceToStep(bandLimit, priceStep, 'up');
      }
    }
    if (!Number.isFinite(price) || price <= 0) {
      throw new Error(`Paradex ${opts.side} market IOC order produced invalid execution price ${price}`);
    }
    return price;
  };

  private buildMainOrderPayload = async (opts: PlaceOrderOpts, clientId?: string) => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };
    if (!this.requestSigner) {
      throw new Error('Cannot place order: Request signer not initialized');
    }
    if (opts.type === OrderType.TrailingStopLoss) {
      return rejectPreflight('Paradex trailing-stop orders are not supported by the current order schema');
    }
    const isOpeningTrigger = opts.type === OrderType.Conditional;
    if (isOpeningTrigger) {
      if (!Number.isFinite(opts.price) || !opts.price || opts.price <= 0) {
        return rejectPreflight('Paradex opening trigger orders require a finite positive execution price');
      }
      if (!Number.isFinite(opts.triggerPrice) || !opts.triggerPrice || opts.triggerPrice <= 0) {
        return rejectPreflight('Paradex opening trigger orders require a finite positive trigger price');
      }
      if (opts.reduceOnly) {
        return rejectPreflight('Paradex opening trigger orders cannot be reduce-only; use a stop-loss order instead');
      }
      if (opts.takeProfit !== undefined || opts.stopLoss !== undefined) {
        return rejectPreflight('Paradex cannot safely attach TP/SL children before an opening trigger order fills');
      }
    }
    const side = opts.side === OrderSide.Buy ? 'BUY' : 'SELL';
    let type = this.mapOrderType(opts.type);
    let executionPrice = opts.price;
    let timeInForce = opts.timeInForce;
    const marketIocExecutionPrice = await this.resolveMarketIocExecutionPrice(opts);
    if (marketIocExecutionPrice !== undefined) {
      type = 'LIMIT';
      executionPrice = marketIocExecutionPrice;
      timeInForce = OrderTimeInForce.ImmediateOrCancel;
    }
    const isStopOrder = isParadexTriggerOrderType(type);
    const hasLimitExecutionPrice = type === 'LIMIT' || type === 'STOP_LIMIT' || type.endsWith('_LIMIT');
    const triggerPrice = isOpeningTrigger ? opts.triggerPrice : isStopOrder ? executionPrice : opts.triggerPrice;
    const signOrderParams: {
      market: string;
      side: 'BUY' | 'SELL';
      type: string;
      size: string;
      price?: string;
    } = {
      market: opts.symbol,
      side,
      type,
      size: opts.amount.toString(),
    };
    if (hasLimitExecutionPrice && executionPrice) {
      signOrderParams.price = executionPrice.toString();
    }
    const { signature, signature_timestamp, headers } = await this.requestSigner.signOrder(signOrderParams);
    const instruction = this.mapTimeInForce(timeInForce);
    const shouldBeReduceOnly = opts.reduceOnly || (isStopOrder && !isOpeningTrigger);

    const payload = omitUndefined({
      market: opts.symbol,
      side,
      type,
      size: opts.amount.toString(),
      price: hasLimitExecutionPrice && executionPrice ? executionPrice.toString() : undefined,
      trigger_price: triggerPrice?.toString(),
      instruction,
      flags: shouldBeReduceOnly ? ['REDUCE_ONLY'] : undefined,
      stp: 'EXPIRE_TAKER',
      signature,
      signature_timestamp,
      client_id: clientId,
    });
    return { payload, headers };
  };

  private placeMainOrder = async (opts: PlaceOrderOpts): Promise<string> => {
    try {
      // Ensure SDK is initialized and request signer is available
      if (this.sdkInitPromise) {
        await this.sdkInitPromise;
      }

      if (!this.requestSigner) {
        throw new Error('Cannot place order: Request signer not initialized');
      }

      this.log('Signing order...', LogSeverity.Debug);
      const clientId = this.resolveCallerClientOrderId(opts) ?? this.generateClientId();
      const { payload, headers } = await this.buildMainOrderPayload(opts, clientId);

      this.log(`Placing order: ${JSON.stringify(payload)}`, LogSeverity.Debug);

      const { data } = await this.xhr.post<ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDER, payload, ...signerHeaders(headers));

      if (!data || typeof data !== 'object' || Array.isArray(data)) {
        const message = 'Paradex order response malformed: expected an object';
        this.emitAndLogError(message);
        throw new Error(message);
      }
      if (typeof data.id !== 'string' || data.id.trim().length === 0) {
        const message = 'Paradex order response malformed: missing canonical order id';
        this.emitAndLogError(message);
        throw new Error(message);
      }
      if (data.client_id !== clientId) {
        const message = `Paradex order response malformed: expected echoed client_id ${clientId}`;
        this.emitAndLogError(message);
        throw new Error(message);
      }

      this.log(`Main order placed: ${data.id}`, LogSeverity.Info);
      return data.id;
    } catch (err: any) {
      this.log(`Error in placeMainOrder: ${err.message}`, LogSeverity.Error);

      // Extract and emit user-friendly error message from API response
      let errorMessage = err.message;
      if (err?.response?.data) {
        this.log(`API error response: ${JSON.stringify(err.response.data)}`, LogSeverity.Error);

        // Check for error.message field (Paradex format)
        if (err.response.data.error && err.response.data.message) {
          errorMessage = `${err.response.data.error}: ${err.response.data.message}`;
        } else if (err.response.data.message) {
          errorMessage = err.response.data.message;
        } else if (err.response.data.error) {
          errorMessage = err.response.data.error;
        }

        // Emit the error for user notification
        this.emitAndLogError(errorMessage);
      }

      throw err;
    }
  };

  private placeNativeBatchChunk = async (orders: readonly PlaceOrderOpts[]): Promise<string[]> => {
    if (orders.length < 1 || orders.length > 10) {
      const message = `Paradex native order batch must contain between 1 and 10 orders; received ${orders.length}`;
      this.emitAndLogError(message);
      throw new Error(message);
    }
    if (this.sdkInitPromise) {
      await this.sdkInitPromise;
    }

    const expectedClientIds: string[] = [];
    const payloads: Awaited<ReturnType<ParadexExchange['buildMainOrderPayload']>>['payload'][] = [];
    let batchHeaders: Record<string, string> | undefined;
    try {
      if (this.options.payloadSigner && orders.length > 1) {
        throw new Error('Paradex batch orders are not supported with payloadSigner, which signs one order per request');
      }
      for (const order of orders) {
        if (!order.symbol || !Number.isFinite(order.amount) || order.amount <= 0) {
          throw new Error('Paradex batch LIMIT order requires a symbol and a finite positive amount');
        }
        if (!Number.isFinite(order.price) || !order.price || order.price <= 0) {
          throw new Error('Paradex batch LIMIT order requires a finite positive price');
        }
      }
      const clientIdSet = new Set<string>();
      for (let index = 0; index < orders.length; index++) {
        const clientId = orders[index].clientOrderId ?? this.generateClientId();
        if (clientIdSet.has(clientId)) {
          throw new Error(`Paradex batch generated an invalid or duplicate client_id at index ${index}`);
        }
        clientIdSet.add(clientId);
        expectedClientIds.push(clientId);
      }
      for (let index = 0; index < orders.length; index++) {
        const { payload, headers } = await this.buildMainOrderPayload(orders[index], expectedClientIds[index]);
        payloads.push(payload);
        batchHeaders = headers;
      }
    } catch (error: any) {
      const message = error?.message || String(error);
      this.emitAndLogError(message);
      throw error;
    }

    let data: BATCH_ORDER_RESPONSE;
    try {
      ({ data } = await this.xhr.post<BATCH_ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDERS_BATCH, payloads, ...signerHeaders(batchHeaders)));
    } catch (error: any) {
      const message = error?.response?.data?.message || error?.response?.data?.error || error?.message || String(error);
      this.emitAndLogError(message);
      throw error;
    }

    const malformed = (detail: string): never => {
      const message = `Paradex batch order response malformed: ${detail}`;
      this.emitAndLogError(message);
      throw new Error(message);
    };
    if (!data || typeof data !== 'object' || Array.isArray(data)) {
      return malformed('expected an object envelope');
    }
    if (data.orders !== undefined && !Array.isArray(data.orders)) {
      return malformed('orders must be an array when present');
    }
    if (data.errors !== undefined && !Array.isArray(data.errors)) {
      return malformed('errors must be an array when present');
    }

    const expected = new Set(expectedClientIds);
    const idsByClientId = new Map<string, string>();
    for (const [index, row] of (data.orders || []).entries()) {
      if (!row || typeof row !== 'object' || Array.isArray(row)) {
        return malformed(`orders[${index}] must be an object`);
      }
      if (typeof row.client_id !== 'string' || !expected.has(row.client_id)) {
        return malformed(`orders[${index}] has an unknown or missing client_id`);
      }
      if (idsByClientId.has(row.client_id)) {
        return malformed(`orders[${index}] repeats client_id ${row.client_id}`);
      }
      if (typeof row.id !== 'string' || row.id.length === 0) {
        return malformed(`orders[${index}] is missing its Paradex order id`);
      }
      idsByClientId.set(row.client_id, row.id);
    }

    const decodedErrors = (data.errors || []).map((row, index) => {
      if (!row || typeof row !== 'object' || Array.isArray(row)) {
        return malformed(`errors[${index}] must be an object`);
      }
      const error = row.error;
      const message = row.message;
      if (
        (error !== undefined && typeof error !== 'string') ||
        (message !== undefined && typeof message !== 'string')
      ) {
        return malformed(`errors[${index}] must contain only string error/message fields`);
      }
      if (!error && !message) {
        return malformed(`errors[${index}] does not contain an error or message`);
      }
      return [error, message].filter(Boolean).join(': ');
    });
    const missingClientIds = expectedClientIds.filter((clientId) => !idsByClientId.has(clientId));

    if (decodedErrors.length > 0) {
      if (missingClientIds.length !== decodedErrors.length) {
        return malformed(
          `${idsByClientId.size} successes plus ${decodedErrors.length} errors do not account for ${expectedClientIds.length} requests`,
        );
      }
      const message = `Paradex batch rejected client IDs [${missingClientIds.join(', ')}]: ${decodedErrors.join('; ')}`;
      this.emitAndLogError(message);
      throw new Error(message);
    }
    if (missingClientIds.length > 0 || idsByClientId.size !== expectedClientIds.length) {
      return malformed(`missing results for client IDs [${missingClientIds.join(', ')}]`);
    }

    return expectedClientIds.map((clientId) => idsByClientId.get(clientId)!);
  };

  private placeTakeProfitStopLoss = async (opts: PlaceOrderOpts, _mainOrderId: string): Promise<string[]> => {
    const orderIds: string[] = [];

    // Paradex does not support infinite TP/SL (zero-sized TP/SL that closes entire position)
    // User must specify explicit order amount for TP/SL orders
    if (!opts.amount || opts.amount === 0) {
      const errorMessage =
        'Paradex does not support infinite TP/SL orders. Please specify an explicit order size for TP/SL.';
      this.log(errorMessage, LogSeverity.Error);
      this.emitAndLogError(errorMessage);
      throw new Error(errorMessage);
    }

    // Validate TP/SL prices make sense
    if (!this.validateTpSlPrices(opts)) {
      this.log('Invalid TP/SL prices, skipping bracket orders', LogSeverity.Warning);
      return orderIds;
    }

    // Ensure request signer is available
    if (!this.requestSigner) {
      this.log('Cannot place TP/SL orders: Request signer not initialized', LogSeverity.Warning);
      return orderIds;
    }

    // Determine the opposite side for TP/SL (they close the position)
    const oppositeSide = opts.side === OrderSide.Buy ? 'SELL' : 'BUY';

    try {
      // Place Take Profit order if specified
      if (opts.takeProfit) {
        // Sign the TP order (price omitted for TAKE_PROFIT_MARKET)
        const {
          signature: tpSignature,
          signature_timestamp: tpTimestamp,
          headers: tpHeaders,
        } = await this.requestSigner.signOrder({
          market: opts.symbol,
          side: oppositeSide,
          type: 'TAKE_PROFIT_MARKET',
          size: opts.amount.toString(),
        });

        const tpPayload = omitUndefined({
          market: opts.symbol,
          side: oppositeSide,
          type: 'TAKE_PROFIT_MARKET',
          size: opts.amount.toString(),
          trigger_price: opts.takeProfit.toString(),
          instruction: 'GTC',
          flags: ['REDUCE_ONLY'],
          signature: tpSignature,
          signature_timestamp: tpTimestamp,
        });

        const { data: tpData } = await this.xhr.post<ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDER, tpPayload, ...signerHeaders(tpHeaders));
        orderIds.push(tpData.id);
        this.log(`Take profit order placed: ${tpData.id} @ ${opts.takeProfit}`, LogSeverity.Info);
      }

      // Place Stop Loss order if specified
      if (opts.stopLoss) {
        // Sign the SL order (price omitted for STOP_LOSS_MARKET)
        const {
          signature: slSignature,
          signature_timestamp: slTimestamp,
          headers: slHeaders,
        } = await this.requestSigner.signOrder({
          market: opts.symbol,
          side: oppositeSide,
          type: 'STOP_LOSS_MARKET',
          size: opts.amount.toString(),
        });

        const slPayload = omitUndefined({
          market: opts.symbol,
          side: oppositeSide,
          type: 'STOP_LOSS_MARKET',
          size: opts.amount.toString(),
          trigger_price: opts.stopLoss.toString(),
          instruction: 'GTC',
          flags: ['REDUCE_ONLY'],
          signature: slSignature,
          signature_timestamp: slTimestamp,
        });

        const { data: slData } = await this.xhr.post<ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDER, slPayload, ...signerHeaders(slHeaders));
        orderIds.push(slData.id);
        this.log(`Stop loss order placed: ${slData.id} @ ${opts.stopLoss}`, LogSeverity.Info);
      }
    } catch (err: any) {
      this.log(`Error placing TP/SL orders: ${err}`, LogSeverity.Warning);

      // Extract and emit error message
      if (err?.response?.data) {
        let errorMessage = err.message;
        if (err.response.data.error && err.response.data.message) {
          errorMessage = `${err.response.data.error}: ${err.response.data.message}`;
        } else if (err.response.data.message) {
          errorMessage = err.response.data.message;
        } else if (err.response.data.error) {
          errorMessage = err.response.data.error;
        }
        this.emitAndLogError(`TP/SL order failed: ${errorMessage}`);
      }

      // Don't throw - main order was already placed successfully
    }

    return orderIds;
  };

  /**
   * Validate that TP/SL prices are logically correct
   * - For Long (Buy): TP should be above entry, SL should be below entry
   * - For Short (Sell): TP should be below entry, SL should be above entry
   */
  private validateTpSlPrices = (opts: PlaceOrderOpts): boolean => {
    const entryPrice = opts.price;
    if (!entryPrice) {
      // For market orders, we can't validate without current price
      return true;
    }

    if (opts.side === OrderSide.Buy) {
      // Long position
      if (opts.takeProfit && opts.takeProfit <= entryPrice) {
        this.log(`Invalid TP for long: ${opts.takeProfit} must be > entry ${entryPrice}`, LogSeverity.Warning);
        return false;
      }
      if (opts.stopLoss && opts.stopLoss >= entryPrice) {
        this.log(`Invalid SL for long: ${opts.stopLoss} must be < entry ${entryPrice}`, LogSeverity.Warning);
        return false;
      }
    } else {
      // Short position
      if (opts.takeProfit && opts.takeProfit >= entryPrice) {
        this.log(`Invalid TP for short: ${opts.takeProfit} must be < entry ${entryPrice}`, LogSeverity.Warning);
        return false;
      }
      if (opts.stopLoss && opts.stopLoss <= entryPrice) {
        this.log(`Invalid SL for short: ${opts.stopLoss} must be > entry ${entryPrice}`, LogSeverity.Warning);
        return false;
      }
    }

    return true;
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    const results: string[] = [];
    let nativeBatch: PlaceOrderOpts[] = [];
    const flushNativeBatch = async () => {
      if (nativeBatch.length === 0) return;
      const batch = nativeBatch;
      nativeBatch = [];
      results.push(...(await this.placeNativeBatchChunk(batch)));
    };

    for (const order of orders) {
      if (this.isNativeBatchOrder(order)) {
        nativeBatch.push(order);
        if (nativeBatch.length === 10) {
          await flushNativeBatch();
        }
        continue;
      }

      await flushNativeBatch();
      try {
        results.push(...(await this.placeOrder(order)));
      } catch (err) {
        this.log(`Error placing order: ${err}`, LogSeverity.Error);
      }
    }
    await flushNativeBatch();
    return results;
  };

  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    const { order, update } = opts;
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    if (!this.requestSigner) {
      return rejectPreflight('Paradex cannot amend an order before the request signer is initialized');
    }

    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) {
      return rejectPreflight(`Paradex cannot safely amend ${order.id}: market ${order.symbol} is not reconciled`);
    }

    const raw = order.info as Partial<ORDER_RESPONSE> | undefined;
    const rawId = raw?.id === undefined || raw?.id === null ? '' : String(raw.id);
    const rawMarket = String(raw?.market || '');
    const rawSide = String(raw?.side || '').toUpperCase();
    const rawType = String(raw?.type || '').toUpperCase();
    const rawStatus = String(raw?.status || '').toUpperCase();
    const expectedSide = order.side === OrderSide.Buy ? 'BUY' : 'SELL';
    const supportedTriggerType = isParadexTriggerOrderType(rawType);
    const supportedLimit = rawType === 'LIMIT' && (rawStatus === 'NEW' || rawStatus === 'OPEN');
    const supportedUntriggeredTpSl = supportedTriggerType && rawStatus === 'UNTRIGGERED';

    if (!supportedLimit && !supportedUntriggeredTpSl) {
      return rejectPreflight(
        `Paradex native amendment supports only exactly reconciled open LIMIT or untriggered TPSL orders; ${rawType || 'unknown'} ${rawStatus || 'unknown'} is unsupported`,
      );
    }
    const rawReduceOnly =
      typeof raw?.reduce_only === 'boolean'
        ? raw.reduce_only
        : Array.isArray(raw?.flags)
          ? raw.flags.includes('REDUCE_ONLY')
          : undefined;
    if (
      rawId !== order.id ||
      rawMarket !== order.symbol ||
      rawSide !== expectedSide ||
      this.reverseMapOrderType(rawType, rawReduceOnly) !== order.type
    ) {
      return rejectPreflight(
        `Paradex cannot safely amend ${order.id}: order identity or immutable shape is not reconciled`,
      );
    }
    if (raw?.on_behalf_of_account) {
      return rejectPreflight(`Paradex cannot safely amend ${order.id}: isolated-margin order routing is unsupported`);
    }

    const rawInstruction = String(raw?.instruction || '').toUpperCase();
    const knownInstruction = ['GTC', 'POST_ONLY', 'IOC', 'RPI'].includes(rawInstruction);
    const hasLegacyInstruction =
      typeof raw?.post_only === 'boolean' || ['GTT', 'FOK', 'IOC'].includes(String(raw?.time_in_force || ''));
    if (!knownInstruction && !hasLegacyInstruction) {
      return rejectPreflight(
        `Paradex cannot safely amend ${order.id}: time-in-force/post-only state is not reconciled`,
      );
    }

    if (rawReduceOnly === undefined || rawReduceOnly !== order.reduceOnly) {
      return rejectPreflight(`Paradex cannot safely amend ${order.id}: reduce-only state is not reconciled`);
    }

    const rawSize = Number(raw?.size);
    const rawRemaining = Number(raw?.remaining_size);
    const rawPrice = Number(raw?.price);
    const rawTriggerPrice = Number(raw?.trigger_price);
    const reconciledFilled = rawSize - rawRemaining;
    if (
      !Number.isFinite(rawSize) ||
      rawSize <= 0 ||
      !Number.isFinite(rawRemaining) ||
      rawRemaining < 0 ||
      rawRemaining > rawSize ||
      rawSize !== order.amount ||
      rawRemaining !== order.remaining ||
      reconciledFilled !== order.filled
    ) {
      return rejectPreflight(
        `Paradex cannot safely amend ${order.id}: total/filled/remaining base size is not reconciled`,
      );
    }
    if (!Number.isFinite(rawPrice) || rawPrice < 0 || (supportedLimit && rawPrice <= 0)) {
      return rejectPreflight(`Paradex cannot safely amend ${order.id}: existing limit price is not reconciled`);
    }
    if (supportedLimit && rawPrice !== order.price) {
      return rejectPreflight(`Paradex cannot safely amend ${order.id}: displayed limit price is not reconciled`);
    }
    if (
      supportedUntriggeredTpSl &&
      (!Number.isFinite(rawTriggerPrice) || rawTriggerPrice <= 0 || rawTriggerPrice !== order.price)
    ) {
      return rejectPreflight(`Paradex cannot safely amend ${order.id}: TPSL trigger price is not reconciled`);
    }

    const amendedSize = adjust('amount' in update ? update.amount : rawSize, market.precision.amount);
    const amendedPrice = supportedLimit
      ? adjust('price' in update ? update.price : rawPrice, market.precision.price)
      : rawPrice;
    const amendedTriggerPrice = supportedUntriggeredTpSl
      ? adjust('price' in update ? update.price : rawTriggerPrice, market.precision.price)
      : undefined;
    if (!Number.isFinite(amendedSize) || amendedSize <= 0) {
      return rejectPreflight('Paradex amended total base size must be greater than zero');
    }
    if (reconciledFilled > 0 && amendedSize <= reconciledFilled) {
      return rejectPreflight('Paradex amended total base size must exceed the already-filled base size');
    }
    if (!Number.isFinite(amendedPrice) || amendedPrice < 0 || (supportedLimit && amendedPrice <= 0)) {
      return rejectPreflight('Paradex amended order price is invalid');
    }
    if (
      supportedUntriggeredTpSl &&
      (amendedTriggerPrice === undefined || !Number.isFinite(amendedTriggerPrice) || amendedTriggerPrice <= 0)
    ) {
      return rejectPreflight('Paradex amended TPSL trigger price must be greater than zero');
    }

    try {
      const signatureFields = {
        id: order.id,
        market: rawMarket,
        side: rawSide as 'BUY' | 'SELL',
        type: rawType,
        size: String(amendedSize),
        price: String(amendedPrice),
      };
      const { signature, signature_timestamp, headers } = await this.requestSigner.signModifyOrder(signatureFields);
      const payload = omitUndefined({
        ...signatureFields,
        trigger_price: amendedTriggerPrice === undefined ? undefined : String(amendedTriggerPrice),
        signature,
        signature_timestamp,
      });
      const { data } = await this.xhr.put<ORDER_RESPONSE>(ENDPOINTS.MODIFY_ORDER.replace('{id}', order.id), payload, ...signerHeaders(headers));
      if (!data || String(data.id || '') !== order.id) {
        throw new Error(`Paradex modify response did not confirm stable order id ${order.id}`);
      }
      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.response?.data?.error || err?.message || String(err));
      throw err;
    }
  };

  cancelOrder = async (orderId: string): Promise<void> => {
    try {
      await this.xhr.delete(ENDPOINTS.CANCEL_ORDER.replace('{id}', orderId));
      this.log(`Cancelled order ${orderId}`, LogSeverity.Debug);
    } catch (err) {
      this.log(`Error canceling order ${orderId}: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  private cancelNativeBatchChunk = async (orderIds: readonly string[]): Promise<void> => {
    if (orderIds.length !== 2) {
      throw new Error(`Paradex native cancel batch requires exactly 2 order IDs; received ${orderIds.length}`);
    }

    const requestedIds = [...orderIds];
    if (requestedIds.some((orderId) => typeof orderId !== 'string' || orderId.trim().length === 0)) {
      throw new Error('Paradex native cancel batch requires two non-empty order IDs');
    }
    if (new Set(requestedIds).size !== requestedIds.length) {
      throw new Error(`Paradex native cancel batch received duplicate order IDs [${requestedIds.join(', ')}]`);
    }

    const { data } = await this.xhr.delete<BATCH_CANCEL_ORDER_RESPONSE>(ENDPOINTS.CANCEL_ORDERS_BATCH, {
      data: { order_ids: requestedIds },
    });

    const malformed = (detail: string): never => {
      throw new Error(`Paradex batch cancel response malformed: ${detail}`);
    };
    if (!data || typeof data !== 'object' || Array.isArray(data)) {
      return malformed('expected an object envelope');
    }
    if (!Array.isArray(data.results)) {
      return malformed('results must be an array');
    }

    const expected = new Set(requestedIds);
    const seen = new Set<string>();
    const rejectedIds: string[] = [];
    const anomalies: string[] = [];

    for (const [index, row] of data.results.entries()) {
      if (!row || typeof row !== 'object' || Array.isArray(row)) {
        anomalies.push(`results[${index}] must be an object`);
        continue;
      }

      const status = row.status;
      const orderId = row.id;
      if (typeof orderId !== 'string' || orderId.trim().length === 0) {
        anomalies.push(
          status === 'NOT_FOUND'
            ? `results[${index}] has uncorrelated NOT_FOUND without an order id`
            : `results[${index}] is missing its order id`,
        );
        continue;
      }
      if (!expected.has(orderId)) {
        anomalies.push(`results[${index}] has unknown order id ${orderId}`);
        continue;
      }
      if (seen.has(orderId)) {
        anomalies.push(`results[${index}] repeats order id ${orderId}`);
        continue;
      }
      seen.add(orderId);

      if (status === 'QUEUED_FOR_CANCELLATION' || status === 'ALREADY_CLOSED') {
        continue;
      }
      if (status === 'NOT_FOUND') {
        rejectedIds.push(orderId);
        continue;
      }
      anomalies.push(`results[${index}] has unsupported status ${String(status)}`);
    }

    if (data.results.length !== requestedIds.length) {
      anomalies.push(`received ${data.results.length} results for ${requestedIds.length} requested IDs`);
    }
    const missingIds = requestedIds.filter((orderId) => !seen.has(orderId));
    if (missingIds.length > 0) {
      anomalies.push(`missing requested IDs [${missingIds.join(', ')}]`);
    }
    if (anomalies.length > 0) {
      return malformed(anomalies.join('; '));
    }
    if (rejectedIds.length > 0) {
      throw new Error(`Paradex batch cancel rejected order IDs [${rejectedIds.join(', ')}]: NOT_FOUND`);
    }

    this.log(`Cancelled order batch [${requestedIds.join(', ')}]`, LogSeverity.Debug);
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    const failures: Array<{ error: unknown; orderIds: string[] }> = [];
    for (let offset = 0; offset < orders.length; offset += 2) {
      const chunk = orders.slice(offset, offset + 2);
      const orderIds = chunk.map((order) => order.id);
      try {
        if (orderIds.length === 2) {
          await this.cancelNativeBatchChunk(orderIds);
        } else {
          await this.cancelOrder(orderIds[0]);
        }
      } catch (error) {
        failures.push({ error, orderIds });
        const message = error instanceof Error ? error.message : String(error);
        this.log(`Error canceling order chunk [${orderIds.join(', ')}]: ${message}`, LogSeverity.Error);
      }
    }

    if (failures.length === 1) {
      throw failures[0].error;
    }
    if (failures.length > 1) {
      const details = failures
        .map(({ error, orderIds }) => {
          const message = error instanceof Error ? error.message : String(error);
          return `[${orderIds.join(', ')}]: ${message}`;
        })
        .join('; ');
      throw new Error(
        `Paradex cancellation failed for ${failures.length} chunks after attempting all orders: ${details}`,
      );
    }
  };

  cancelAllOrders = async (): Promise<void> => {
    try {
      await this.xhr.delete(ENDPOINTS.CANCEL_ALL_ORDERS);
      this.log('Cancelled all orders', LogSeverity.Info);
    } catch (err) {
      this.log(`Error canceling all orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    try {
      // DELETE /orders?market={symbol}
      await this.xhr.delete(ENDPOINTS.CANCEL_ALL_ORDERS, { params: { market: symbol } });
      this.log(`Cancelled all orders for ${symbol}`, LogSeverity.Info);
    } catch (err) {
      this.log(`Error canceling symbol orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchLeverageSettings = async (): Promise<void> => {
    try {
      this.log('Fetching leverage settings from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<MARGIN_RESPONSE>(ENDPOINTS.ALL_MARGINS);

      if (!data || !data.configs) {
        this.log('Margin API returned invalid data', LogSeverity.Warning);
        return;
      }

      // Update leverageSettings map with all configured leverages
      let updatedCount = 0;
      for (const config of data.configs) {
        if (config.leverage > 0) {
          this.leverageSettings.set(config.market, config.leverage);
          updatedCount++;
        }
      }

      this.log(`Updated leverage settings for ${updatedCount} markets`, LogSeverity.Debug);
    } catch (err) {
      this.log(`Error fetching leverage settings: ${err}`, LogSeverity.Error);
      // Don't throw - leverage settings are not critical, we have fallback defaults
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number): Promise<void> => {
    try {
      // Validate market exists
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        const errorMsg = `Market ${symbol} not found`;
        this.log(errorMsg, LogSeverity.Error);
        throw new Error(errorMsg);
      }

      // Clamp leverage to market limits
      const minLeverage = market.limits.leverage?.min || 1;
      const maxLeverage = market.limits.leverage?.max || 100;
      const leverage = Math.min(Math.max(Math.floor(inputLeverage), minLeverage), maxLeverage);

      if (leverage !== inputLeverage) {
        this.log(
          `Leverage ${inputLeverage} adjusted to ${leverage} (min: ${minLeverage}, max: ${maxLeverage})`,
          LogSeverity.Warning,
        );
      }

      this.log(`Setting leverage for ${symbol} to ${leverage}x`, LogSeverity.Info);

      // Paradex API: POST /account/margin/{market}
      // Body requires: leverage (int), margin_type (CROSS/ISOLATED)
      const endpoint = ENDPOINTS.SET_MARGIN.replace('{market}', symbol);
      const payload = {
        leverage,
        margin_type: 'CROSS',
      };

      this.log(`POST ${endpoint} with payload: ${JSON.stringify(payload)}`, LogSeverity.Debug);

      const response = await this.xhr.post(endpoint, payload);

      this.log(`Response status: ${response.status}, data: ${JSON.stringify(response.data)}`, LogSeverity.Debug);

      // Store leverage setting in our map
      this.leverageSettings.set(symbol, leverage);

      // Update leverage in local positions immediately (optimistic update)
      // This is necessary because Paradex doesn't return leverage for positions with 0 size
      const existingPositions = this.store.positions.filter((p) => p.symbol === symbol);
      if (existingPositions.length > 0) {
        // Update existing positions with new leverage
        const updatedPositions = existingPositions.map((p) => ({
          ...p,
          leverage,
        }));
        this.store.addOrUpdatePositions(updatedPositions);
        this.log(`Updated leverage in existing positions for ${symbol}`, LogSeverity.Debug);
      } else {
        // Create fake position if it doesn't exist
        // This ensures leverage is available even before opening a position
        // For one-way mode, position ID is just the symbol
        const fakePosition: Position = {
          id: symbol,
          symbol,
          side: PositionSide.Long,
          entryPrice: 0,
          notional: 0,
          leverage,
          unrealizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
        };
        this.store.addOrUpdatePositions([fakePosition]);
        this.log(`Created fake position with leverage ${leverage}x for ${symbol}`, LogSeverity.Debug);
      }

      // Note: Error responses (including proxy 200s with error bodies) are handled by the axios interceptor
      this.log(`Successfully set leverage for ${symbol} to ${leverage}x`, LogSeverity.Info);
    } catch (err: any) {
      // Error message is already formatted by the axios interceptor
      const errorMsg = err?.message || 'Unknown error';
      this.log(`Error setting leverage for ${symbol}: ${errorMsg}`, LogSeverity.Error);
      this.log(`Full error object: ${JSON.stringify(err, Object.getOwnPropertyNames(err))}`, LogSeverity.Debug);
      if (err?.response) {
        this.log(`Error response data: ${JSON.stringify(err.response.data)}`, LogSeverity.Debug);
        this.log(`Error response status: ${err.response.status}`, LogSeverity.Debug);
      }
      this.emitAndLogError(`Failed to set leverage for ${symbol}: ${errorMsg}`);
      throw err;
    }
  };

  changeMarginType = async (isolated: boolean) => {
    if (isolated) this.rejectPreflight('Paradex isolated account routing is unsupported');
  };

  changePositionMode = async (hedged: boolean) => {
    if (hedged) this.rejectPreflight('Paradex supports one-way mode only');
  };

  setMaxSlippage = async (maxSlippage: string, market: string): Promise<void> => {
    try {
      if (!market) {
        throw new Error('Paradex max slippage requires a market symbol');
      }

      this.log(`Setting account max slippage for ${market} to ${maxSlippage}`, LogSeverity.Info);

      const endpoint = ENDPOINTS.MAX_SLIPPAGE.replace('{market}', market);
      const payload = {
        max_slippage: maxSlippage,
      };

      this.log(`POST ${endpoint} with payload: ${JSON.stringify(payload)}`, LogSeverity.Debug);

      const response = await this.xhr.post(endpoint, payload);

      this.log(`Response status: ${response.status}, data: ${JSON.stringify(response.data)}`, LogSeverity.Debug);
      this.log(`Successfully set account max slippage for ${market} to ${maxSlippage}`, LogSeverity.Info);
    } catch (err: any) {
      const errorMsg = err?.message || 'Unknown error';
      this.log(`Error setting max slippage: ${errorMsg}`, LogSeverity.Error);
      this.log(`Full error object: ${JSON.stringify(err, Object.getOwnPropertyNames(err))}`, LogSeverity.Debug);
      if (err?.response) {
        this.log(`Error response data: ${JSON.stringify(err.response.data)}`, LogSeverity.Debug);
        this.log(`Error response status: ${err.response.status}`, LogSeverity.Debug);
      }
      this.emitAndLogError(`Failed to set max slippage: ${errorMsg}`);
      throw err;
    }
  };

  private mapOrder = (o: ORDER_RESPONSE): Order => {
    const side = ORDER_SIDE[o.side];
    const reduceOnly =
      typeof o.reduce_only === 'boolean'
        ? o.reduce_only
        : Array.isArray(o.flags)
          ? o.flags.includes('REDUCE_ONLY')
          : false;
    const type = this.reverseMapOrderType(o.type, reduceOnly);
    const status = this.mapOrderStatus(o.status);

    // For stop orders, use trigger_price as the price (since _MARKET types have no limit price)
    const isStopOrder = isParadexTriggerOrderType(o.type);
    const displayPrice =
      isStopOrder && o.trigger_price ? parseFloat(o.trigger_price) : o.price ? parseFloat(o.price) : 0;

    return {
      id: o.id,
      symbol: o.market,
      side,
      type,
      status,
      price: displayPrice,
      amount: parseFloat(o.size),
      filled: parseFloat(o.size) - parseFloat(o.remaining_size),
      remaining: parseFloat(o.remaining_size),
      reduceOnly,
      clientOrderId: o.client_id || o.client_order_id || undefined,
      timeInForce: paradexTimeInForce(o),
      lastUpdate: new Date(o.created_at).getTime(),
      info: o, // Store all raw data from API
    };
  };

  private mapTimeInForce = (timeInForce?: string): string => {
    // Paradex expects: GTC, IOC, POST_ONLY
    if (!timeInForce) return 'GTC';

    const mapping: Record<string, string> = {
      GoodTillCancel: 'GTC',
      GTC: 'GTC',
      ImmediateOrCancel: 'IOC',
      IOC: 'IOC',
      PostOnly: 'POST_ONLY',
      POST_ONLY: 'POST_ONLY',
      GTT: 'GTC', // Good Till Time -> Good Till Cancel
      FOK: 'IOC', // Fill Or Kill -> Immediate Or Cancel (closest match)
    };

    return mapping[timeInForce] || 'GTC';
  };

  private mapOrderType = (type: OrderType): string => {
    const mapping: Record<OrderType, string> = {
      [OrderType.Market]: 'MARKET',
      [OrderType.Limit]: 'LIMIT',
      [OrderType.Conditional]: 'STOP_LIMIT',
      [OrderType.StopLoss]: 'STOP_MARKET',
      [OrderType.TakeProfit]: 'TAKE_PROFIT_MARKET',
      [OrderType.TrailingStopLoss]: 'STOP_MARKET',
    };
    return mapping[type] || 'LIMIT';
  };

  private reverseMapOrderType = (type: string, reduceOnly = true): OrderType => {
    if (!reduceOnly && (type === 'STOP_LIMIT' || type === 'STOP_MARKET')) {
      return OrderType.Conditional;
    }
    return ORDER_TYPE[type] || OrderType.Limit;
  };

  private mapOrderStatus = (status: string): OrderStatus => {
    const mapping: Record<string, OrderStatus> = {
      NEW: OrderStatus.Open,
      OPEN: OrderStatus.Open,
      PENDING: OrderStatus.Open,
      UNTRIGGERED: OrderStatus.Open,
      FILLED: OrderStatus.Closed,
      CANCELED: OrderStatus.Canceled,
      REJECTED: OrderStatus.Canceled,
    };
    return mapping[status] || OrderStatus.Open;
  };
}
