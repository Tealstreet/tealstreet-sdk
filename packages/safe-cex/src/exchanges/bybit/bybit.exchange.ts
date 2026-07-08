import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';
import type { ExchangeName } from '../../types';

import axios from 'axios';
import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import debounce from 'lodash/debounce';
import omit from 'lodash/omit';
import orderBy from 'lodash/orderBy';
import times from 'lodash/times';
import { forEachSeries, mapSeries } from 'p-iteration';

import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Balance,
  Candle,
  ClosedPnlEntry,
  CoinBalance,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  OHLCVSOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { mergeUpdatesUsingLastUpdate } from '../../utils/mergeUpdatesUsingLastUpdate';
import { omitUndefined } from '../../utils/omitUndefined';
import { add, adjust, subtract } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createAPI } from './bybit.api';
import { BYBIT_RATE_LIMIT_TABLE, BybitAccountType, BybitProductType, BybitVipLevel } from './bybit.rateLimits';
import { getBybitResponseMeta } from './bybit.response';
import {
  BybitApiKeyInfo,
  BybitTransactionHistoryOptions,
  BybitTransactionRecord,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TIME_IN_FORCE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
} from './bybit.types';
import { BybitPrivateWebsocket } from './bybit.ws-private';
import { BybitPublicWebsocket } from './bybit.ws-public';
import { syncUnifiedBalanceUpnlFromPositions } from './syncUnifiedBalanceUpnlFromPositions';

const INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1_000;
const WS_DEGRADED_SNAPSHOT_DEBOUNCE_MS = 250;
const WS_DEGRADED_SNAPSHOT_MAX_WAIT_MS = 1_200;
const BYBIT_POSITION_TPSL_MODES = new Set(['Full', 'Partial']);

const isBybitPositionTpslMode = (mode: unknown) => typeof mode === 'string' && BYBIT_POSITION_TPSL_MODES.has(mode);

const getBybitTriggerBy = (opts: PlaceOrderOpts) => {
  if (opts.type === OrderType.Conditional) {
    return 'MarkPrice';
  }
  if (opts.type === OrderType.StopLoss) {
    return opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'LastPrice' : 'MarkPrice';
  }
  if (opts.type === OrderType.TakeProfit) {
    return opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MarkPrice' : 'LastPrice';
  }
  return undefined;
};

export class BybitExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BybitPublicWebsocket;
  privateWebsocket: BybitPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private unifiedMarginStatus: number = 1;
  private marginType: MarginType = MarginType.Cross;

  private needsReinit = false;
  private readonly debouncedDegradedSnapshotRefresh = debounce(
    async () => {
      if (this.isDisposed || this.isPublicDataOnly) {
        return;
      }

      try {
        await this.fetchAndSetPrivateSnapshotHelper();
      } catch (err: any) {
        this.emitAndLogError(err);
      }
    },
    WS_DEGRADED_SNAPSHOT_DEBOUNCE_MS,
    { maxWait: WS_DEGRADED_SNAPSHOT_MAX_WAIT_MS },
  );

  positionMode: PositionMode | null = null;

  lastWebsocketPositionsUpdate: number | undefined;

  private apiKeyInfo: BybitApiKeyInfo | null = null;

  private batchLimit: number = 10;
  private readonly EXECUTION_QUERY_MAX_RANGE_MS = 7 * 24 * 60 * 60 * 1000 - 1;
  private readonly DEFAULT_EXECUTIONS_LOOKBACK_MS = 7 * 24 * 60 * 60 * 1000;

  private getExecutionQueryWindows = (
    from?: number,
    to?: number,
  ): Array<{
    startTime?: number;
    endTime?: number;
  }> => {
    if (from === undefined || to === undefined) {
      return [{ startTime: from, endTime: to }];
    }

    if (to - from <= this.EXECUTION_QUERY_MAX_RANGE_MS) {
      return [{ startTime: from, endTime: to }];
    }

    const windows: Array<{ startTime: number; endTime: number }> = [];
    let windowEnd = to;
    while (windowEnd > from) {
      const windowStart = Math.max(from, windowEnd - this.EXECUTION_QUERY_MAX_RANGE_MS);
      windows.push({ startTime: windowStart, endTime: windowEnd });
      if (windowStart <= from) {
        break;
      }
      windowEnd = windowStart - 1;
    }

    return windows;
  };

  get accountType() {
    return this.unifiedMarginStatus === 1 ? 'CONTRACT' : 'UNIFIED';
  }

  get cachedApiKeyInfo() {
    return this.apiKeyInfo;
  }

  getSettleCoinsForCategory(category: string) {
    if (category === 'linear') {
      if (this.accountType === 'UNIFIED') return ['USDT', 'USDC'];
      return ['USDT'];
    }
    this.emitAndLogError(`Unknown category ${category}`);
    throw new Error(`Unknown category ${category}`);
  }

  getCategoryForSymbol = (_symbol: string) => {
    return 'linear';
  };

  getTickerSymbolsForCategory(category: 'linear' | 'inverse') {
    const marketSymbols = this.store.markets
      .filter((market) => {
        if (category === 'linear') {
          return market.quote === 'USDT' || market.quote === 'USDC';
        }

        return market.quote !== 'USDT' && market.quote !== 'USDC';
      })
      .map((market) => market.symbol);

    return this.getTickerSymbolsForSubscription(marketSymbols);
  }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BybitPublicWebsocket(this, 'linear');
    this.privateWebsocket = new BybitPrivateWebsocket(this);
  }

  dispose = () => {
    this.debouncedDegradedSnapshotRefresh.cancel();
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  private shouldUseDegradedPrivateRestFastPath = () => {
    if (this.isPublicDataOnly) {
      return false;
    }

    const ws = this.privateWebsocket;
    if (!ws) {
      return false;
    }

    // Prefer snappier REST reconciliation when WS is in a reconnect pause window
    // or when it initialized previously but is now unhealthy.
    return ws.isReconnectPauseActive || (ws.isInitialized && !ws.isHealthy);
  };

  private scheduleDegradedPrivateSnapshotRefresh = () => {
    if (!this.shouldUseDegradedPrivateRestFastPath()) {
      return;
    }

    this.debouncedDegradedSnapshotRefresh();
  };

  getAccount = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT);

    if (data.retCode !== 0) {
      throw new Error(data.retMsg);
    }

    return {
      userId: data?.result?.userID,
      affiliateId: data?.result?.inviterID,
    };
  };

  validateAccount = async () => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_MARGIN);

      if (data.retMsg !== 'OK') {
        this.emitAndLogError(data.retMsg);

        if (data.retMsg.includes('timestamp and recv_window param')) {
          return { error: 'Check your computer time and date' };
        }

        return { error: data.retMsg };
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }

    // Key validated above; balance is best-effort for the new-account gate (a
    // separate wallet-balance call) and never fails validation.
    let balance: Balance | undefined;
    try {
      balance = await this.fetchBalance();
    } catch {
      // best-effort
    }
    return { error: '', balance };
  };

  start = async () => {
    await virtualClock.start();

    // Reset batch limit on exchange start
    this.batchLimit = 10;

    const publicDataOnly = this.isPublicDataOnly;

    const useExternalPublicData = await this.initExternalPublicData();

    // Fetch API key info asynchronously without blocking startup
    if (!publicDataOnly) {
      this.fetchAndCacheApiKeyInfo().catch(() => {
        // Already handled in fetchAndCacheApiKeyInfo
      });
    }

    if (!useExternalPublicData) {
      // Fetch markets, tickers, and margin info in parallel
      const [_, rawTickers] = await Promise.all([
        this.fetchMarkets().then((markets) => {
          if (this.isDisposed) return [];
          this.log(`Loaded ${markets.length} Bybit markets`);
          this.store.update({
            markets,
            loaded: { ...this.store.loaded, markets: true },
          });

          // start public websocket streams as soon as markets loaded.
          this.publicWebsocket.connectAndSubscribe();

          return markets;
        }),
        this.fetchTickersWithoutParse(),
        ...(!publicDataOnly ? [this.fetchMarginAccountInfos()] : []),
      ]);

      const tickers = this.parseTickers(rawTickers);

      if (this.isDisposed) return;

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });
    }

    this.log(`Ready to trade on Bybit`);

    // start ticking live data
    // balance, tickers, positions
    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    // Additional safety check to prevent private websocket connection for demo/empty credentials
    // (remoteSigner mode has empty secret but still needs private WS)
    if (!this.options.key || (!this.options.secret && !this.options.remoteSigner)) {
      this.log('Skipping private websocket connection - no API credentials', LogSeverity.Warning);
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    await this.fetchOrdersPositionsAndBalance();

    this.tick();
  };

  fetchOrdersPositionsAndBalance = async () => {
    try {
      const [, , rawBalance] = await Promise.all([
        this.fetchOrders().then((orders) => {
          if (this.isDisposed) return [];
          this.store.update({
            orders,
            loaded: { ...this.store.loaded, orders: true },
          });
          return orders;
        }),
        this.fetchPositions().then((positions) => {
          if (this.isDisposed) return [];
          this.store.addOrUpdatePositionsFull(positions);
          this.store.update({
            loaded: { ...this.store.loaded, positions: true },
          });
          return positions;
        }),
        this.fetchRawBalance(),
      ]);

      if (this.isDisposed) return;
      const balance = this.formatBalance(rawBalance);
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err);
    }
  };

  fetchAndSetPrivateSnapshot = async () => {
    await this.fetchAndSetPrivateSnapshotHelper();
  };

  fetchAndSetPrivateSnapshotHelper = async (fetchOrders = true) => {
    try {
      if (fetchOrders) {
        const orders = await this.fetchOrders();
        if (this.isDisposed) return;

        this.store.update({
          orders,
          loaded: { ...this.store.loaded, orders: true },
        });
      }

      const fetchedPositions = await this.fetchPositions();
      if (this.isDisposed) return;
      const positions =
        this.lastWebsocketPositionsUpdate !== undefined &&
        virtualClock.getCurrentTimeMs() - this.lastWebsocketPositionsUpdate <
          INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY
          ? mergeUpdatesUsingLastUpdate(fetchedPositions, this.store.positions)
          : fetchedPositions;

      this.store.addOrUpdatePositionsFull(positions);
      this.store.update({
        loaded: { ...this.store.loaded, positions: true },
      });

      const balance = await this.fetchBalance();

      if (this.isDisposed) return;
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err);
    }
  };

  fetchMarginAccountInfos = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_MARGIN);

    if (data.retMsg !== 'OK') {
      this.emitAndLogError(data.retMsg);
      return;
    }

    this.marginType = data?.result?.marginMode === 'ISOLATED_MARGIN' ? MarginType.Isolated : MarginType.Cross;
    this.unifiedMarginStatus = data?.result?.unifiedMarginStatus;
  };

  tick = async () => {
    if (!this.isDisposed) {
      // Wait for remote signer if configured but unavailable
      if (!(await this.waitForRemoteSigner())) return;

      if (this.needsReinit) {
        await this.fetchOrdersPositionsAndBalance();
        this.needsReinit = false;
      }

      try {
        await this.fetchAndSetPrivateSnapshotHelper(false);
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };

  fetchBalance = async () => {
    const rawBalance = await this.fetchRawBalance();
    if (this.isDisposed) return this.store.balance;
    return this.formatBalance(rawBalance);
  };

  syncBalanceUpnlFromPositions = () => {
    const balance = syncUnifiedBalanceUpnlFromPositions(this.store.balance, this.store.positions, this.accountType);
    if (!balance || balance === this.store.balance) {
      return;
    }

    this.store.update({
      balance,
    });
  };

  // for UTA 2.0
  forceUnifiedBalance = false;

  fetchRawBalance = async (): Promise<any> => {
    const { data } = await this.xhr.get(ENDPOINTS.BALANCE, {
      params: { accountType: this.forceUnifiedBalance ? 'UNIFIED' : this.accountType },
    });

    if (v(data, 'retMsg') !== 'OK' && +v(data, 'retCode') !== 0) {
      if (v(data, 'retMsg') === 'accountType only support UNIFIED.' && !this.forceUnifiedBalance) {
        this.forceUnifiedBalance = true;
        this.unifiedMarginStatus = 6;
        return this.fetchRawBalance();
      }
      this.emitAndLogError(v(data, 'retMsg'));
      return this.store.balance?.info || [];
    }
    return data.result.list;
  };

  formatBalance = (rawBalance: any) => {
    // UNIFIED ACCOUNT TYPE BALANCE CALCULATION
    // ----------------------------------------
    if (this.accountType === 'UNIFIED') {
      // Ensure rawBalance is an array before destructuring
      if (!Array.isArray(rawBalance) || rawBalance.length === 0) {
        this.log('Invalid or empty balance data received', LogSeverity.Warning);
        return this.store.balance;
      }
      const [firstAccount] = rawBalance;

      const upnl = this.store.positions.reduce((acc, p) => acc + p.unrealizedPnl, 0);
      const total = subtract(parseFloat(firstAccount.totalEquity), upnl);

      const balance: Balance = {
        total,
        upnl,
        used: parseFloat(firstAccount.totalMaintenanceMargin) + parseFloat(firstAccount.totalInitialMargin),
        free: parseFloat(firstAccount.totalAvailableBalance ?? firstAccount.totalMarginBalance),
        coins: firstAccount.coin.reduce(
          (prev: { [coin: string]: CoinBalance }, coin: any) => {
            prev[coin.coin] = {
              base: coin.coin,
              amount: parseFloat(coin.equity),
              notional: parseFloat(coin.usdValue),
            };
            return prev;
          },
          {} as { [coin: string]: CoinBalance },
        ),
        info: firstAccount,
      };

      return balance;
    }

    // NORMAL ACCOUNT TYPE BALANCE CALCULATION
    // ---------------------------------------
    // Ensure rawBalance is an array before destructuring
    if (!Array.isArray(rawBalance) || rawBalance.length === 0) {
      this.log('Invalid or empty balance data received', LogSeverity.Warning);
      return this.store.balance;
    }
    const [firstAccount] = rawBalance;
    const usdt = firstAccount?.coin?.find?.((c: any) => c.coin === 'USDT');

    // The user has no USDT balance, yet?
    if (!usdt) return this.store.balance;

    const balance: Balance = {
      total: parseFloat(usdt.walletBalance),
      upnl: parseFloat(usdt.unrealisedPnl),
      used: add(parseFloat(usdt.totalOrderIM), parseFloat(usdt.totalPositionIM)),
      free: parseFloat(usdt.availableToWithdraw),
    };

    return balance;
  };

  fetchApiKeyInfo = async (): Promise<BybitApiKeyInfo> => {
    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT);

    if (+v(data, 'retCode') !== 0) {
      this.emitAndLogError(v(data, 'retMsg'));
      throw new Error(v(data, 'retMsg'));
    }

    // Store the API key info locally
    this.apiKeyInfo = data.result;

    return data.result;
  };

  private fetchAndCacheApiKeyInfo = async (): Promise<void> => {
    try {
      await this.fetchApiKeyInfo();
      if (this.apiKeyInfo) {
        this.updateBatchLimitBasedOnVipLevel();
        this.store.updateUserData({
          bybitApiKeyInfo: this.apiKeyInfo,
        });
      }
      this.log(
        `API key info cached. VIP Level: ${this.apiKeyInfo?.vipLevel || 'Unknown'}, Batch Limit: ${this.batchLimit}`,
      );
    } catch {
      // Don't fail if the API key info fetch fails
      this.log('Failed to fetch API key info, continuing without it');
    }
  };

  private updateBatchLimitBasedOnVipLevel = (): void => {
    if (!this.apiKeyInfo?.vipLevel) {
      return;
    }

    // Find the rate limit entry for this VIP level
    const rateLimitEntry = BYBIT_RATE_LIMIT_TABLE.find(
      (entry) => entry.level === (this.apiKeyInfo!.vipLevel as BybitVipLevel),
    );
    if (!rateLimitEntry) {
      this.log(`No rate limit entry found for VIP level: ${this.apiKeyInfo.vipLevel}`);
      return;
    }

    // Get the futures rate limit based on account type
    const accountType = this.accountType === 'UNIFIED' ? BybitAccountType.UTAPro : BybitAccountType.ClassicAndUTA;
    const futuresRateLimit = rateLimitEntry[BybitProductType.Futures][accountType];

    // Update batch limit if rate limit is >= 20
    if (futuresRateLimit >= 20) {
      this.batchLimit = 20;
      this.log(
        `VIP level ${this.apiKeyInfo.vipLevel} allows ${futuresRateLimit} req/s for futures. Setting batch limit to 20.`,
      );
    } else {
      this.log(
        `VIP level ${this.apiKeyInfo.vipLevel} allows ${futuresRateLimit} req/s for futures. Keeping batch limit at ${this.batchLimit}.`,
      );
    }
  };

  fetchOrders = async () => {
    const settleCoins = this.getSettleCoinsForCategory('linear');
    const orders = await mapSeries(settleCoins, async (settleCoin) => {
      return this.fetchOrdersHelper('linear', settleCoin);
    });
    return orders.flat();
  };

  fetchOrdersHelper = async (category: string, settleCoin?: string) => {
    const recursiveFetch = async (
      cursor: string = '',
      orders: Array<Record<string, any>> = [],
    ): Promise<Array<Record<string, any>>> => {
      const { data } = await this.xhr.get(ENDPOINTS.UNFILLED_ORDERS, {
        params: {
          category,
          settleCoin,
          limit: 50,
          cursor,
        },
      });

      if (v(data, 'retMsg') !== 'OK') {
        this.emitAndLogError(v(data, 'retMsg'));
        this.needsReinit = true;
        throw new Error(v(data, 'retMsg'));
      }

      const ordersList = Array.isArray(data?.result?.list) ? data.result.list : [];

      if (ordersList.length === 0) {
        return orders;
      }

      if (data.result.nextPageCursor) {
        return recursiveFetch(data.result.nextPageCursor, [...orders, ...ordersList]);
      }

      return [...orders, ...ordersList];
    };

    const bybitOrders = await recursiveFetch();
    const orders: Order[] = bybitOrders.flatMap(this.mapOrder.bind(this));

    return orders;
  };

  fetchPositions = async (symbols?: string[]) => {
    if (symbols && symbols?.length > 0) {
      return await this.fetchPositionsHelper(symbols);
    } else {
      const settleCoins = this.getSettleCoinsForCategory('linear');
      const positions = await mapSeries(settleCoins, async (settleCoin) => {
        return this.fetchPositionsHelper(undefined, 'linear', settleCoin);
      });
      return positions.flat();
    }
  };

  // TODO: fetch recursively to handle > 200 positions
  fetchPositionsHelper = async (symbols?: string[], category?: string, settleCoin?: string) => {
    if (symbols && symbols.length > 0) {
      const positions: Position[][] = await mapSeries(symbols, async (symbol) => {
        const { data } = await this.xhr.get(ENDPOINTS.POSITIONS, {
          params: {
            symbol,
            category: this.getCategoryForSymbol(symbol),
            limit: 200,
          },
        });
        if (v(data, 'retMsg') !== 'OK') {
          this.emitAndLogError(v(data, 'retMsg'));
          throw new Error(v(data, 'retMsg'));
        }
        return data.result.list.map(this.mapPosition.bind(this));
      });
      return positions.flat();
    } else {
      const { data } = await this.xhr.get(ENDPOINTS.POSITIONS, {
        params: {
          category,
          settleCoin,
          limit: 200,
        },
      });
      if (v(data, 'retMsg') !== 'OK') {
        this.emitAndLogError(v(data, 'retMsg'));
        this.needsReinit = true;
        return [];
      }
      const positions: Position[] = data.result.list.map(this.mapPosition.bind(this));
      return positions;
    }
  };

  fetchTickersWithoutParse = async () => {
    const rawTickers = await this.fetchTickersRaw();
    if (this.isDisposed) return [];
    return rawTickers;
  };

  fetchTickers = async () => {
    const rawTickers = await this.fetchTickersRaw();
    if (this.isDisposed) return this.store.tickers;
    return this.parseTickers(rawTickers);
  };

  fetchTickersRaw = async () => {
    const tickers = await this.fetchTickersRawHelper('linear');
    if (tickers.some((t: any) => t === null)) {
      this.emitAndLogError('Failed to fetch tickers');
      return null;
    }
    return tickers;
  };

  fetchTickersRawHelper = async (category: string) => {
    const { data } = await this.xhr.get(ENDPOINTS.TICKERS, {
      params: { category },
    });

    if (v(data, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data, 'retMsg'));
      return [null];
    }

    return data.result.list;
  };

  parseTickers = (rawTickers: any) => {
    if (!rawTickers) return this.store.tickers;

    // Build via indexed for-loop + push instead of `reduce + [...acc, t]`.
    // The prior reduce-with-spread shape was O(N²): for 600 tickers it
    // copied ~180K element references just to build the result array.
    const tickers: Ticker[] = [];
    for (let i = 0; i < rawTickers.length; i++) {
      const t = rawTickers[i];
      const market = this.store.marketsBySymbol[t.symbol];
      if (!market) continue;

      const lastPrice = parseFloat(t.lastPrice);
      const volume24h = parseFloat(t.volume24h);
      tickers.push({
        id: market.id,
        symbol: market.symbol,
        bid: parseFloat(t.bid1Price),
        ask: parseFloat(t.ask1Price),
        last: lastPrice,
        mark: parseFloat(t.markPrice),
        index: parseFloat(t.indexPrice),
        percentage: parseFloat(t.price24hPcnt) * 100,
        openInterest: parseFloat(t.openInterest),
        fundingRate: market.isExpiring ? 0 : parseFloat(t.fundingRate),
        fundingTime: market.isExpiring ? 0 : parseFloat(t.nextFundingTime),
        volume: volume24h,
        quoteVolume: volume24h * lastPrice,
      });
    }
    return tickers;
  };

  fetchMarkets = async () => {
    const markets = await this.fetchMarketsHelper('linear');
    if (markets.some((m) => m === null)) {
      this.emitAndLogError('Failed to fetch markets');
      return this.store.markets;
    }
    return markets as Market[];
  };

  fetchFees = async (
    category: string,
  ): Promise<
    ReadonlyArray<{
      symbol: string;
      takerFeeRate: string;
      makerFeeRate: string;
    }>
  > => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.FEE_RATE, {
        params: { category },
      });
      return data?.result?.list || [];
    } catch (_e) {
      return [];
    }
  };

  fetchMarketsHelper = async (category: string) => {
    const fees = this.isPublicDataOnly ? [] : await this.fetchFees(category);

    const [{ data }, { data: data2 }] = await Promise.all([
      this.xhr.get(ENDPOINTS.MARKETS, {
        params: { category, limit: 1000 },
      }),
      this.xhr.get(ENDPOINTS.MARKETS, {
        params: { category, limit: 1000, status: 'PreLaunch' },
      }),
    ]);

    if (v(data, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data, 'retMsg'));
      return [null];
    }

    if (v(data2, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data2, 'retMsg'));
      return [null];
    }

    const markets: Market[] = [...data.result.list, ...data2.result.list].map((market: Record<string, any>) => {
      const fee = fees.find((f) => f.symbol === market.symbol);
      return {
        // old id isn't unique with expiring usdc futs
        // id: `${market.baseCoin}/${market.quoteCoin}:${market.settleCoin}`,
        id: market.symbol,
        symbol: market.symbol,
        base: market.baseCoin,
        quote: market.quoteCoin,
        active: market.status === 'Trading' || market.status === 'PreLaunch',
        precision: {
          amount: parseFloat(market.lotSizeFilter.qtyStep),
          price: parseFloat(market.priceFilter.tickSize),
        },
        limits: {
          amount: {
            min: parseFloat(market.lotSizeFilter.minOrderQty),
            max: parseFloat(market.lotSizeFilter.maxOrderQty),
          },
          leverage: {
            min: parseFloat(market.leverageFilter.minLeverage),
            max: parseFloat(market.leverageFilter.maxLeverage),
          },
        },
        isExpiring: Boolean(parseFloat(market.deliveryTime)),
        ...(fee
          ? {
              takerFee: parseFloat(fee.takerFeeRate),
              makerFee: parseFloat(fee.makerFeeRate),
            }
          : {}),
        info: market,
      };
    });

    return markets;
  };

  fetchClosedPnl? = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (process.env.NEXT_PUBLIC_DISABLE_EXTRA_APIS_IN_DEV === '1') {
      return [];
    }
    const baseUrl = typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '';
    return (
      await axios.get(
        `${baseUrl}/api/trades-history/bybit/closed-pnl?accountName=` + opts.accountName + '&symbol=' + opts.symbol,
      )
    ).data;
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 500, 1000);
    const interval = INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const params = {
      category: this.getCategoryForSymbol(opts.symbol),
      symbol: opts.symbol,
      start: start.valueOf(),
      end: end.valueOf(),
      interval,
      limit,
    };

    console.warn(
      `[BYBIT-KLINE] adapter=${typeof this.xhr.defaults.adapter} remoteSigner=${!!this.options.remoteSigner} url=${ENDPOINTS.KLINE}`,
    );
    const response = await this.xhr.get(ENDPOINTS.KLINE, { params });
    const { data } = response;

    // Validate response — without this, errors silently return empty candle arrays.
    // data can be null (transport/timeout issues) or have retCode !== 0 (Bybit API errors).
    if (!data) {
      console.error(
        `[BYBIT-KLINE] null data! response keys=${Object.keys(response).join(',')} status=${response.status} adapter=${typeof this.xhr.defaults.adapter}`,
      );
      throw new Error(`Bybit KLINE: null response data (status=${response.status})`);
    }
    if (data.retCode !== undefined && data.retCode !== 0) {
      throw new Error(`Bybit KLINE error: ${data.retMsg || 'unknown'} (retCode=${data.retCode})`);
    }
    if (!data.result?.list) {
      throw new Error(`Bybit KLINE: no candle list in response (keys=${Object.keys(data).join(',')})`);
    }

    const candles: Candle[] = orderBy(
      data.result.list.map(([open_time, open, high, low, close, volume]: string[]) => {
        return {
          timestamp: parseFloat(open_time) / 1000,
          open: parseFloat(open),
          high: parseFloat(high),
          low: parseFloat(low),
          close: parseFloat(close),
          volume: parseFloat(volume),
        };
      }),
      ['timestamp'],
      ['asc'],
    );

    return candles;
  };

  private mapOrderHistoryRowsToExecutions = (
    rows: Array<Record<string, any>>,
    symbol: string,
    orderId?: string,
    from?: number,
    to?: number,
  ): Execution[] => {
    const parseTimestampMs = (raw: any): number | undefined => {
      const value = Number(raw);
      if (!Number.isFinite(value) || value <= 0) return undefined;
      return value > 100_000_000_000 ? value : value * 1000;
    };

    const parseFeeFromDetail = (detail: any): number | undefined => {
      if (!detail) return undefined;
      let parsed = detail;
      if (typeof detail === 'string') {
        try {
          parsed = JSON.parse(detail);
        } catch {
          return undefined;
        }
      }
      if (!parsed || typeof parsed !== 'object') return undefined;

      let feeSum = 0;
      for (const value of Object.values(parsed)) {
        const fee = Number.parseFloat(String(value));
        if (Number.isFinite(fee)) {
          feeSum += Math.abs(fee);
        }
      }
      return feeSum > 0 ? feeSum : undefined;
    };

    const mapped: Execution[] = [];
    for (const row of rows) {
      const rowOrderId = `${row.orderId ?? ''}`;
      if (orderId && rowOrderId && rowOrderId !== `${orderId}`) {
        continue;
      }

      const rowStatus = `${row.orderStatus ?? ''}`.toLowerCase();
      if (rowStatus && !rowStatus.includes('filled')) {
        continue;
      }

      const qty = Number.parseFloat(`${row.cumExecQty ?? row.execQty ?? row.qty ?? '0'}`);
      if (!Number.isFinite(qty) || qty <= 0) {
        continue;
      }

      let price = Number.parseFloat(`${row.avgPrice ?? row.avgFilledPrice ?? row.price ?? '0'}`);
      if (!Number.isFinite(price) || price <= 0) {
        const cumExecValue = Number.parseFloat(`${row.cumExecValue ?? row.cumExecAmt ?? row.cumExecAmount ?? '0'}`);
        if (Number.isFinite(cumExecValue) && cumExecValue > 0 && qty > 0) {
          price = cumExecValue / qty;
        }
      }
      if (!Number.isFinite(price) || price <= 0) {
        continue;
      }

      const side = `${row.side ?? ''}`.toLowerCase();
      const isBuy = side === 'buy';
      const isSell = side === 'sell';
      if (!isBuy && !isSell) {
        continue;
      }

      const timestampMs = parseTimestampMs(row.updatedTime ?? row.createdTime ?? row.execTime ?? row.transactionTime);
      if (!timestampMs) {
        continue;
      }
      if (from !== undefined && timestampMs < from) {
        continue;
      }
      if (to !== undefined && timestampMs > to) {
        continue;
      }

      const directFee = Number.parseFloat(`${row.cumExecFee ?? row.execFee ?? row.fee ?? ''}`);
      const commission = Number.isFinite(directFee) ? Math.abs(directFee) : parseFeeFromDetail(row.cumFeeDetail);

      mapped.push(
        omitUndefined({
          timestamp: Math.floor(timestampMs / 1000),
          symbol: row.symbol || symbol,
          orderId: rowOrderId || undefined,
          price,
          qty,
          side: isBuy ? OrderSide.Buy : OrderSide.Sell,
          commission,
          commissionAsset: row.feeCurrency || row.feeCcy || row.settleCoin || row.quoteCoin || undefined,
        }),
      );
    }

    const dedupedByKey = new Map<string, Execution>();
    for (const execution of mapped) {
      const dedupeKey = `${execution.orderId || ''}:${execution.timestamp}:${execution.side}:${execution.price}:${execution.qty}`;
      if (!dedupedByKey.has(dedupeKey)) {
        dedupedByKey.set(dedupeKey, execution);
      }
    }

    return orderBy(Array.from(dedupedByKey.values()), ['timestamp'], ['desc']);
  };

  private fetchExecutionsFallbackFromOrderHistory = async (
    opts: ExecutionsOptions,
    fallbackLookbackMs: number = this.DEFAULT_EXECUTIONS_LOOKBACK_MS,
  ): Promise<Execution[]> => {
    const { symbol, orderId, from, to, limit = 50 } = opts;
    if (!symbol) return [];

    const fallbackTo = to ?? virtualClock.getCurrentTimeMs();
    const fallbackFrom = from ?? (to === undefined ? fallbackTo - fallbackLookbackMs : undefined);
    const executions: Execution[] = [];
    const MAX_RECURSION_DEPTH = 5;
    const queryWindows = this.getExecutionQueryWindows(fallbackFrom, fallbackTo);
    const recursiveFetch = async (
      queryWindow: { startTime?: number; endTime?: number },
      cursor?: string,
      depth: number = 0,
    ): Promise<void> => {
      if (depth >= MAX_RECURSION_DEPTH) {
        return;
      }

      const params: Record<string, any> = {
        category: this.getCategoryForSymbol(symbol),
        symbol,
        limit: Math.min(limit || 50, 100),
      };
      if (orderId) params.orderId = orderId;
      if (queryWindow.startTime !== undefined) params.startTime = queryWindow.startTime;
      if (queryWindow.endTime !== undefined) params.endTime = queryWindow.endTime;
      if (cursor) params.cursor = cursor;

      const { data } = await this.xhr.get(ENDPOINTS.USER_ORDERS, { params });
      const retMsg = `${v(data, 'retMsg') || 'UNKNOWN'}`;
      if (retMsg !== 'OK') {
        this.log(`Failed order-history fallback fetch executions: ${retMsg}`, LogSeverity.Warning);
        return;
      }

      const rows = Array.isArray(data?.result?.list) ? data.result.list : [];
      executions.push(...this.mapOrderHistoryRowsToExecutions(rows, symbol, orderId, from, to));

      if (data?.result?.nextPageCursor && executions.length < (limit || 50)) {
        await recursiveFetch(queryWindow, data.result.nextPageCursor, depth + 1);
      }
    };

    for (const queryWindow of queryWindows) {
      if (executions.length >= (limit || 50)) {
        break;
      }
      await recursiveFetch(queryWindow);
    }
    return orderBy(executions, ['timestamp'], ['desc']).slice(0, limit || 50);
  };

  fetchExecutions = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 50 } = opts;

    if (!symbol) {
      // Bybit requires symbol for execution queries
      return [];
    }

    const executions: Execution[] = [];
    let primaryRowsCount = 0;
    const MAX_RECURSION_DEPTH = 5;
    const queryWindows = this.getExecutionQueryWindows(from, to);
    const recursiveFetch = async (
      queryWindow: { startTime?: number; endTime?: number },
      cursor?: string,
      depth: number = 0,
    ): Promise<void> => {
      // Limit recursion depth to prevent excessive API calls
      if (depth >= MAX_RECURSION_DEPTH) {
        this.log(`fetchExecutions reached max recursion depth of ${MAX_RECURSION_DEPTH}`, LogSeverity.Warning);
        return;
      }

      const params: Record<string, any> = {
        category: this.getCategoryForSymbol(symbol),
        symbol,
        limit: Math.min(limit || 50, 100),
      };

      if (orderId) {
        params.orderId = orderId;
      }
      if (queryWindow.startTime !== undefined) {
        params.startTime = queryWindow.startTime;
      }
      if (queryWindow.endTime !== undefined) {
        params.endTime = queryWindow.endTime;
      }
      if (cursor) {
        params.cursor = cursor;
      }

      try {
        const { data } = await this.xhr.get(ENDPOINTS.USER_TRADES, { params });
        const retMsg = `${v(data, 'retMsg') || 'UNKNOWN'}`;
        if (retMsg !== 'OK') {
          this.log(`Failed to fetch executions: ${retMsg}`, LogSeverity.Warning);
          return;
        }

        const executionsList = Array.isArray(data?.result?.list) ? data.result.list : [];
        primaryRowsCount += executionsList.length;

        for (const exec of executionsList) {
          // Bybit USER_TRADES can include non-fill rows (Funding/Settle/Delivery).
          // Keep chart/table fills strictly to execution rows that represent actual trades.
          // Use a blacklist for known non-trade rows instead of a strict whitelist to stay
          // resilient if Bybit introduces new trade-like execType values.
          const execType = `${exec.execType ?? ''}`.trim().toLowerCase();
          const isKnownNonTradeExecution =
            execType === 'funding' ||
            execType === 'settle' ||
            execType === 'settlement' ||
            execType === 'delivery' ||
            execType === 'sessionsettlepnl';
          if (isKnownNonTradeExecution) {
            continue;
          }

          const qty = parseFloat(exec.execQty ?? exec.closedSize ?? exec.qty ?? '0');
          let price = parseFloat(exec.execPrice ?? exec.avgPrice ?? exec.price ?? '0');
          if (!Number.isFinite(price) || price <= 0) {
            const execValue = parseFloat(exec.execValue ?? exec.closedPnl ?? '0');
            if (Number.isFinite(execValue) && execValue > 0 && Number.isFinite(qty) && qty > 0) {
              price = execValue / qty;
            }
          }
          const side = `${exec.side ?? exec.orderSide ?? ''}`.trim().toLowerCase();
          const isBuy = side === 'buy';
          const isSell = side === 'sell';
          if (!Number.isFinite(qty) || !Number.isFinite(price) || qty <= 0 || price <= 0 || (!isBuy && !isSell)) {
            continue;
          }

          const rawExecTime = Number(exec.execTime ?? exec.tradeTime ?? exec.updatedTime ?? exec.createdTime);
          if (!Number.isFinite(rawExecTime) || rawExecTime <= 0) {
            continue;
          }
          const timestamp = rawExecTime > 100_000_000_000 ? Math.floor(rawExecTime / 1000) : Math.floor(rawExecTime);

          executions.push({
            id: exec.execId,
            timestamp,
            symbol: exec.symbol || symbol,
            orderId: exec.orderId,
            price,
            qty,
            side: isBuy ? OrderSide.Buy : OrderSide.Sell,
            commission: Math.abs(parseFloat(exec.execFee ?? exec.fee ?? '0')),
            commissionAsset: exec.feeCurrency || exec.feeCoin || exec.quoteCoin,
          });
        }

        // Continue fetching if there's more data and we haven't reached the limit
        if (data?.result?.nextPageCursor && executions.length < (limit || 50)) {
          await recursiveFetch(queryWindow, data.result.nextPageCursor, depth + 1);
        }
      } catch (error: any) {
        this.log(`Error fetching executions: ${error.message}`, LogSeverity.Warning);
        return;
      }
    };

    for (const queryWindow of queryWindows) {
      if (executions.length >= (limit || 50)) {
        break;
      }
      await recursiveFetch(queryWindow);
    }
    if (executions.length > 0) {
      return executions;
    }

    try {
      const fallbackWarning = `fetchExecutions primary path produced no valid fills for ${symbol}; falling back to ${ENDPOINTS.USER_ORDERS} (rawRows=${primaryRowsCount}, windows=${queryWindows.length}, limit=${limit}, hasOrderId=${Boolean(orderId)})`;
      this.log(fallbackWarning, LogSeverity.Warning);
      if (typeof console !== 'undefined' && typeof console.warn === 'function') {
        console.warn(`[${this.name}] ${fallbackWarning}`);
      }
      const fallbackExecutions = await this.fetchExecutionsFallbackFromOrderHistory(opts);
      if (fallbackExecutions.length > 0 || from !== undefined || orderId) {
        return fallbackExecutions;
      }

      const EXTENDED_FALLBACK_LOOKBACK_MS = 30 * 24 * 60 * 60 * 1000;
      if (to === undefined && this.DEFAULT_EXECUTIONS_LOOKBACK_MS < EXTENDED_FALLBACK_LOOKBACK_MS) {
        const extendedWarning = `fetchExecutions short fallback returned no rows for ${symbol}; retrying ${Math.floor(
          EXTENDED_FALLBACK_LOOKBACK_MS / (24 * 60 * 60 * 1000),
        )}d order-history lookup for recovery`;
        this.log(extendedWarning, LogSeverity.Warning);
        if (typeof console !== 'undefined' && typeof console.warn === 'function') {
          console.warn(`[${this.name}] ${extendedWarning}`);
        }
        return await this.fetchExecutionsFallbackFromOrderHistory(opts, EXTENDED_FALLBACK_LOOKBACK_MS);
      }

      if (fallbackExecutions.length === 0) {
        this.log(`fetchExecutions fallback returned no rows for ${symbol}`, LogSeverity.Warning);
      }
      return fallbackExecutions;
    } catch (fallbackErr: any) {
      this.log(
        `Failed order-history fallback fetch executions: ${fallbackErr?.message || fallbackErr}`,
        LogSeverity.Warning,
      );
      return executions;
    }
  };

  /**
   * Fetch income/transaction history from Bybit.
   * Maps Bybit transaction log to the generic IncomeRecord format.
   *
   * @param opts - Options for filtering (symbol, type, time range)
   * @returns Array of income records
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    try {
      const params: BybitTransactionHistoryOptions = {
        // Transaction log endpoint only supports UNIFIED account type
        accountType: 'UNIFIED',
        category: 'linear',
        limit: opts?.limit || 50,
      };

      if (opts?.startTime) {
        params.startTime = opts.startTime;
      }
      if (opts?.endTime) {
        params.endTime = opts.endTime;
      }
      if (opts?.incomeType) {
        // Map generic income types to Bybit transaction types
        const typeMap: Record<string, string> = {
          REALIZED_PNL: 'TRADE',
          FUNDING_FEE: 'SETTLEMENT',
          COMMISSION: 'TRADE',
        };
        params.type = (typeMap[opts.incomeType] || opts.incomeType) as any;
      }

      const allRecords: IncomeRecord[] = [];
      let cursor: string | undefined;
      const maxPages = 10; // Safety limit
      let page = 0;

      do {
        const requestParams = { ...params, cursor };

        const { data } = await this.xhr.get(ENDPOINTS.TRANSACTION_LOG, { params: requestParams });

        if (data.retMsg !== 'OK' && +data.retCode !== 0) {
          this.log(`Failed to fetch transaction log: ${data.retMsg}`, LogSeverity.Error);
          break;
        }

        const records = data.result?.list as BybitTransactionRecord[] | undefined;
        if (!records || records.length === 0) break;

        for (const record of records) {
          // Map Bybit transaction to generic IncomeRecord
          // cashFlow represents the actual P&L change
          const income = parseFloat(record.cashFlow || '0');

          // Map Bybit types to generic income types
          let incomeType = record.type;
          if (record.type === 'TRADE' && income !== 0) {
            incomeType = 'REALIZED_PNL' as any;
          } else if (record.type === 'SETTLEMENT') {
            incomeType = 'FUNDING_FEE' as any;
          }

          allRecords.push({
            symbol: record.symbol || '',
            incomeType,
            income,
            asset: record.currency || 'USDT',
            timestamp: parseInt(record.transactionTime, 10),
            tranId: record.tradeId || record.orderId || `${record.transactionTime}`,
            tradeId: record.tradeId || undefined,
          });
        }

        cursor = data.result?.nextPageCursor;
        page++;
      } while (cursor && page < maxPages && allRecords.length < (opts?.limit || 1000));

      return allRecords;
    } catch (error: any) {
      this.log(`Error fetching income history: ${error.message}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Fetch income history for a date range.
   * Bybit limits queries to 7 days per request, so we chunk the range.
   *
   * @param startTime - Start timestamp in ms
   * @param endTime - End timestamp in ms
   * @param incomeType - Optional filter by income type
   * @returns Array of all income records in the range
   */
  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    const allRecords: IncomeRecord[] = [];
    const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000; // 7 days per chunk (Bybit limit)

    let currentStart = startTime;

    while (currentStart < endTime) {
      const currentEnd = Math.min(currentStart + SEVEN_DAYS_MS, endTime);

      const records = await this.fetchIncomeHistory({
        startTime: currentStart,
        endTime: currentEnd,
        incomeType,
        limit: 1000,
      });

      allRecords.push(...records);

      currentStart = currentEnd;

      // Add a small delay to respect rate limits
      await sleep(100);
    }

    return allRecords;
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOHLCVs = (
    opts: OHLCVSOptions,
    callback: (candle: Candle & { symbol: string; interval: Timeframe }) => void,
  ) => {
    this.publicWebsocket.listenOHLCVs(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  _hasFullSizedPositionOppositeSide = (symbol: string, side: OrderSide, amount: number) => {
    return side === OrderSide.Buy
      ? this.store.positions.some(
          (p) => p.symbol === symbol && p.side === PositionSide.Short && p.contracts !== 0 && p.contracts >= amount,
        )
      : this.store.positions.some(
          (p) => p.symbol === symbol && p.side === PositionSide.Long && p.contracts !== 0 && p.contracts >= amount,
        );
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    if (
      this._hasFullSizedPositionOppositeSide(opts.symbol, opts.side, opts.amount) &&
      (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)
    ) {
      return this.placeStopLossOrTakeProfit(opts);
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      return this.placeTrailingStopLoss(opts);
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const orderPositionOpts = opts.type === OrderType.Conditional ? { ...opts, reduceOnly: false } : opts;
    const positionIdx = await this.getOrderPositionIdx(orderPositionOpts);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const price = opts.price ? adjust(opts.price, pPrice) : null;
    const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
    const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;
    const timeInForce = inverseObj(ORDER_TIME_IN_FORCE)[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
    const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;

    let triggerDirection: number | undefined = undefined;
    if (triggerPrice) {
      const ticker = this.store.tickersBySymbol[opts.symbol]!;
      const priceToCompare =
        (opts.type === OrderType.StopLoss && opts.slTriggerType === OrderStopTriggerType.LastPrice) ||
        (opts.type === OrderType.TakeProfit && opts.tpTriggerType === OrderStopTriggerType.LastPrice)
          ? ticker.last
          : ticker.mark;
      if (triggerPrice > priceToCompare) {
        triggerDirection = 1;
      } else {
        triggerDirection = 2;
      }
    }

    let type = opts.type;
    if (opts.type === OrderType.Conditional) {
      if (!triggerPrice) {
        throw new Error(`Trigger price is required for conditional Bybit orders (${opts.symbol})`);
      }
      type = price ? OrderType.Limit : OrderType.Market;
    }
    if (triggerPrice && (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)) {
      type = OrderType.Market;
    }

    const req = omitUndefined({
      category: this.getCategoryForSymbol(opts.symbol),
      symbol: opts.symbol,
      side: inverseObj(ORDER_SIDE)[opts.side],
      orderType: inverseObj(ORDER_TYPE)[type],
      qty: `${amount}`,
      price: opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price) ? `${price}` : undefined,
      stopLoss: opts.stopLoss ? `${stopLoss}` : undefined,
      takeProfit: opts.takeProfit ? `${takeProfit}` : undefined,
      reduceOnly: opts.type === OrderType.Conditional ? false : opts.reduceOnly || false,
      triggerPrice: opts.triggerPrice ? `${triggerPrice}` : undefined,
      triggerDirection: triggerDirection ? `${triggerDirection}` : undefined,
      triggerBy: getBybitTriggerBy(opts),
      slTriggerBy: opts.stopLoss
        ? opts.slTriggerType === OrderStopTriggerType.LastPrice
          ? 'LastPrice'
          : 'MarkPrice'
        : undefined,
      tpTriggerBy: opts.takeProfit
        ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
          ? 'MarkPrice'
          : 'LastPrice'
        : undefined,
      timeInForce:
        opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price) ? timeInForce : undefined,
      closeOnTrigger: opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit,
      positionIdx,
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);

    const payloads = times(lots, (idx) => {
      // We want to remove stopLoss and takeProfit from the rest of the orders
      // because they are already set on the first one
      const payload = idx > 0 ? omit(req, ['stopLoss', 'takeProfit', 'slTriggerBy', 'tpTriggerBy']) : req;

      return { ...payload, qty: `${lotSize}` };
    });

    if (rest) payloads.push({ ...req, qty: `${rest}` });

    const responses = await mapSeries(payloads, async (p) => {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.CREATE_ORDER, p);
        return data;
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.retMsg || err.message);
        return undefined;
      }
    });

    const fullfilled = responses.filter((r) => r !== undefined);

    let errMsg = '';
    fullfilled.forEach((resp) => {
      if (v(resp, 'retMsg') !== 'OK') {
        this.emitAndLogError(v(resp, 'retMsg'));
        errMsg = v(resp, 'retMsg');
      }
    });

    if (errMsg !== '') {
      throw new Error(errMsg);
    }

    const orderIds = fullfilled.map((resp) => resp.result.orderId);
    this.scheduleDegradedPrivateSnapshotRefresh();
    return orderIds;
  };

  placeStopLossOrTakeProfit = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);

    const calculatedAmount = amount && amount !== 0 ? amount : undefined;

    const payload: Record<string, any> = {
      category: this.getCategoryForSymbol(opts.symbol),
      symbol: opts.symbol,
      positionIdx: await this.getStopOrderPositionIdx(opts),
      tpslMode: calculatedAmount ? 'Partial' : 'Full',
    };

    if (opts.type === OrderType.StopLoss) {
      payload.stopLoss = `${opts.price}`;
      payload.slTriggerBy = opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'LastPrice' : 'MarkPrice';

      if (calculatedAmount) {
        payload.slSize = `${calculatedAmount}`;
      }
    }

    if (opts.type === OrderType.TakeProfit) {
      payload.takeProfit = `${opts.price}`;
      payload.tpTriggerBy = opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MarkPrice' : 'LastPrice';

      if (calculatedAmount) {
        payload.tpSize = `${calculatedAmount}`;
      }
    }

    const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

    if (data.retMsg !== 'OK') {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }

    // SET_TRADING_STOP endpoint doesn't return an orderId
    // Return an empty array as the order is directly applied to the position
    this.scheduleDegradedPrivateSnapshotRefresh();
    return [];
  };

  placeTrailingStopLoss = async (opts: PlaceOrderOpts) => {
    const ticker = this.store.tickersBySymbol[opts.symbol];
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!ticker || !market) {
      throw new Error(`Ticker ${opts.symbol} not found`);
    }

    const distance = adjust(
      Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
      market.precision.price,
    );

    const payload: Record<string, any> = {
      category: this.getCategoryForSymbol(opts.symbol),
      symbol: opts.symbol,
      positionIdx: await this.getStopOrderPositionIdx(opts),
      trailingStop: `${distance}`,
    };

    const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

    if (data.retMsg !== 'OK') {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }

    // SET_TRADING_STOP endpoint doesn't return an orderId
    // Return an empty array as the order is directly applied to the position
    this.scheduleDegradedPrivateSnapshotRefresh();
    return [];
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    if (orders.length === 0) {
      return [];
    }

    // Separate orders that need special handling
    const specialOrders: PlaceOrderOpts[] = [];
    const regularOrders: PlaceOrderOpts[] = [];

    for (const order of orders) {
      // Check if order needs special handling (stop loss/take profit with existing position or trailing stop)
      if (
        (this._hasFullSizedPositionOppositeSide(order.symbol, order.side, order.amount) &&
          (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit)) ||
        order.type === OrderType.TrailingStopLoss
      ) {
        specialOrders.push(order);
      } else {
        regularOrders.push(order);
      }
    }

    const allOrderIds: string[] = [];

    // Handle special orders individually using existing placeOrder method
    for (const order of specialOrders) {
      try {
        const orderIds = await this.placeOrder(order);
        allOrderIds.push(...orderIds);
      } catch (err: any) {
        this.emitAndLogError(`Failed to place special order: ${err.message}`);
        // Continue with other orders instead of failing completely
      }
    }

    // Handle regular orders using batch API
    if (regularOrders.length > 0) {
      const batchOrderIds = await this.placeBatchOrders(regularOrders);
      allOrderIds.push(...batchOrderIds);
    }

    this.scheduleDegradedPrivateSnapshotRefresh();
    return allOrderIds;
  };

  private placeBatchOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    // Group orders by category for batch processing
    const ordersByCategory = orders.reduce(
      (acc, order) => {
        const category = this.getCategoryForSymbol(order.symbol);
        if (!acc[category]) acc[category] = [];
        acc[category].push(order);
        return acc;
      },
      {} as Record<string, PlaceOrderOpts[]>,
    );

    const allOrderIds: string[] = [];

    // Process each category separately
    for (const [category, categoryOrders] of Object.entries(ordersByCategory)) {
      const categoryOrderIds = await this.placeBatchOrdersForCategory(category, categoryOrders);
      allOrderIds.push(...categoryOrderIds);
    }

    return allOrderIds;
  };

  private placeBatchOrdersForCategory = async (category: string, orders: PlaceOrderOpts[]): Promise<string[]> => {
    // Bybit rate limit: max 10 orders per second, so we limit batches to 10 orders
    // even though the API technically supports up to 20 orders per batch
    const batches = chunk(orders, this.batchLimit);
    const allOrderIds: string[] = [];

    for (let i = 0; i < batches.length; i++) {
      const batch = batches[i];

      // Rate limiting: Add 1 second delay between batches to respect 10 orders/second limit
      if (i > 0) {
        await sleep(1_200);
      }

      try {
        const batchOrderIds = await this.processBatch(category, batch);
        allOrderIds.push(...batchOrderIds);
      } catch (err: any) {
        this.emitAndLogError(`Failed to place batch orders: ${err.message}`);
        // Fallback to individual order placement for this batch
        for (const order of batch) {
          try {
            const orderIds = await this.placeOrder(order);
            allOrderIds.push(...orderIds);
          } catch (individualErr: any) {
            this.emitAndLogError(`Failed to place individual order: ${individualErr.message}`);
          }
        }
      }
    }

    return allOrderIds;
  };

  private processBatch = async (category: string, orders: PlaceOrderOpts[]): Promise<string[]> => {
    // Convert orders to batch request format
    const batchRequests = await Promise.all(orders.map(async (order) => await this.formatOrderForBatch(order)));

    // Filter out any null requests (orders that couldn't be formatted)
    const validRequests = batchRequests.filter((req): req is Record<string, any> => req !== null);

    if (validRequests.length === 0) {
      return [];
    }

    const payload = {
      category,
      // Keep TP/SL on every request so scaled batches can attach one TP/SL pair per order.
      request: validRequests,
    };

    const { data } = await this.xhr.post(ENDPOINTS.CREATE_BATCH_ORDER, payload);

    if (data.retMsg !== 'OK') {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }

    // Check for individual order errors
    const errors: string[] = [];
    const orderIds: string[] = [];

    if (data.retExtInfo?.list) {
      data.retExtInfo.list.forEach((extInfo: any, index: number) => {
        if (extInfo.code !== 0) {
          errors.push(`Order ${index + 1}: ${extInfo.msg}`);
        }
      });
    }

    if (data.result?.list) {
      data.result.list.forEach((result: any) => {
        if (result.orderId) {
          orderIds.push(result.orderId);
        }
      });
    }

    if (errors.length > 0) {
      this.emitAndLogError(`Some orders failed: ${errors.join(', ')}`);
    }

    return orderIds;
  };

  private formatOrderForBatch = async (opts: PlaceOrderOpts): Promise<Record<string, any> | null> => {
    try {
      const market = this.store.marketsBySymbol[opts.symbol];

      if (!market) {
        this.emitAndLogError(`Market ${opts.symbol} not found`);
        return null;
      }

      const orderPositionOpts = opts.type === OrderType.Conditional ? { ...opts, reduceOnly: false } : opts;
      const positionIdx = await this.getOrderPositionIdx(orderPositionOpts);

      const pPrice = market.precision.price;
      const pAmount = market.precision.amount;

      const amount = adjust(opts.amount, pAmount);
      const price = opts.price ? adjust(opts.price, pPrice) : null;
      const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
      const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;
      const timeInForce = inverseObj(ORDER_TIME_IN_FORCE)[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
      const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;

      let triggerDirection: number | undefined = undefined;
      if (triggerPrice) {
        const ticker = this.store.tickersBySymbol[opts.symbol]!;
        const priceToCompare =
          (opts.type === OrderType.StopLoss && opts.slTriggerType === OrderStopTriggerType.LastPrice) ||
          (opts.type === OrderType.TakeProfit && opts.tpTriggerType === OrderStopTriggerType.LastPrice)
            ? ticker.last
            : ticker.mark;
        if (triggerPrice > priceToCompare) {
          triggerDirection = 1;
        } else {
          triggerDirection = 2;
        }
      }

      let type = opts.type;
      if (opts.type === OrderType.Conditional) {
        if (!triggerPrice) {
          throw new Error(`Trigger price is required for conditional Bybit orders (${opts.symbol})`);
        }
        type = price ? OrderType.Limit : OrderType.Market;
      }
      if (triggerPrice && (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)) {
        type = OrderType.Market;
      }

      const req = omitUndefined({
        symbol: opts.symbol,
        side: inverseObj(ORDER_SIDE)[opts.side],
        orderType: inverseObj(ORDER_TYPE)[type],
        qty: `${amount}`,
        price: opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price) ? `${price}` : undefined,
        stopLoss: opts.stopLoss ? `${stopLoss}` : undefined,
        takeProfit: opts.takeProfit ? `${takeProfit}` : undefined,
        reduceOnly: opts.type === OrderType.Conditional ? false : opts.reduceOnly || false,
        triggerPrice: opts.triggerPrice ? `${triggerPrice}` : undefined,
        triggerDirection: triggerDirection ? `${triggerDirection}` : undefined,
        triggerBy: getBybitTriggerBy(opts),
        slTriggerBy: opts.stopLoss
          ? opts.slTriggerType === OrderStopTriggerType.LastPrice
            ? 'LastPrice'
            : 'MarkPrice'
          : undefined,
        tpTriggerBy: opts.takeProfit
          ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
            ? 'MarkPrice'
            : 'LastPrice'
          : undefined,
        timeInForce:
          opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price) ? timeInForce : undefined,
        closeOnTrigger: opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit,
        positionIdx,
      });

      return req;
    } catch (err: any) {
      this.emitAndLogError(`Failed to format order for batch: ${err.message}`);
      return null;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    // Virtual TP/SL are bracket orders attached to a parent (e.g. limit order); we update via REPLACE_ORDER on parent, not cancel+place
    const isVirtualSLorTP = order.id.endsWith('__stop_loss') || order.id.endsWith('__take_profit');

    if (order.type === OrderType.TrailingStopLoss) {
      await this.cancelOrders([order]);
      return this.placeTrailingStopLoss({ ...order, ...update });
    }

    // Cancel+replace for non-virtual, position-bound TP/SL (created via SET_TRADING_STOP).
    // These have tpslMode set to 'Full' or 'Partial' in their info.
    // Free-floating conditional stops (no tpslMode) are handled via REPLACE_ORDER below.
    // projectedType ensures we re-place with correct TP vs SL based on new price vs mark,
    // avoiding duplicate-order issues with Bybit AMEND/EDIT.
    if (
      !isVirtualSLorTP &&
      (order.type === OrderType.TakeProfit || order.type === OrderType.StopLoss) &&
      isBybitPositionTpslMode(order.info?.tpslMode)
    ) {
      const mark = this.store.tickersBySymbol[order.symbol]?.mark;
      if (mark) {
        const newPrice = 'price' in update ? update.price : order.price;
        const projectedType =
          order.side === OrderSide.Sell
            ? newPrice > mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss
            : newPrice <= mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss;
        await this.cancelOrders([order]);
        return this.placeStopLossOrTakeProfit({ ...order, ...update, type: projectedType });
      }
    }

    const updatedOrderIds = [] as string[];

    // Special use-case when position is not open yet
    // and we want to update the stop loss or take profit
    // we need to do it on the original ask/bid order

    let payload: Record<string, any> = {};

    if ('price' in update && isVirtualSLorTP) {
      const og = this.store.orders.find(
        (o) => o.id === order.id.replace('__stop_loss', '').replace('__take_profit', ''),
      );

      if (og) {
        const key = order.id.endsWith('__stop_loss') ? 'stopLoss' : 'takeProfit';

        payload = {
          category: this.getCategoryForSymbol(order.symbol),
          orderId: og.id,
          symbol: order.symbol,
          [key]: `${update.price}`,
        };

        try {
          const { data } = await this.xhr.post(ENDPOINTS.REPLACE_ORDER, payload);

          if (data.retMsg === 'OK') {
            updatedOrderIds.push(data.result.orderId);
          } else if (data.retMsg === 'order not modified') {
            // do nothing, it's ok
          } else {
            this.emitAndLogError(data.retMsg);
            throw new Error(data.retMsg);
          }
        } catch (err: any) {
          this.emitAndLogError(err, err?.response?.data?.retMsg || err.message);
          throw err;
        }
      }
    }

    // Free-floating conditional stops: non-virtual SL/TP without tpslMode.
    // Position-bound stops have tpslMode set and are handled above via cancel+replace.
    const isConditionalOrder =
      !isVirtualSLorTP &&
      (order.type === OrderType.StopLoss ||
        order.type === OrderType.TakeProfit ||
        order.type === OrderType.Conditional) &&
      !isBybitPositionTpslMode(order.info?.tpslMode);

    // If we want to update the price or amount of a limit order
    // we can do it directly on the order
    if (order.type === OrderType.Limit || isConditionalOrder) {
      payload = {
        category: this.getCategoryForSymbol(order.symbol),
        orderId: order.id,
        symbol: order.symbol,
      };

      if ('amount' in update) payload.qty = `${update.amount}`;
      if ('price' in update) {
        if (isConditionalOrder) {
          payload.triggerPrice = `${update.price}`;
        } else {
          payload.price = `${update.price}`;
        }
      }

      const { data } = await this.xhr.post(ENDPOINTS.REPLACE_ORDER, payload);

      if (data.retMsg === 'OK') {
        updatedOrderIds.push(data.result.orderId);
      } else if (data.retMsg === 'order not modified') {
        // do nothing, it's ok
      } else {
        this.emitAndLogError(data.retMsg);
      }
    }

    // Fallback for position-bound TP/SL when cancel+replace couldn't run
    // (e.g., mark price unavailable). Includes tpslMode and size for partial stops.
    if (
      !isConditionalOrder &&
      !isVirtualSLorTP &&
      (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit)
    ) {
      const isPartial = order.amount !== 0;
      payload = {
        category: this.getCategoryForSymbol(order.symbol),
        symbol: order.symbol,
        positionIdx: await this.getOrderPositionIdx(order),
        tpslMode: isPartial ? 'Partial' : 'Full',
      };

      if ('price' in update) {
        if (order.type === OrderType.StopLoss) {
          payload.stopLoss = `${update.price}`;
        } else if (order.type === OrderType.TakeProfit) {
          payload.takeProfit = `${update.price}`;
        }
      }

      if (isPartial) {
        const size = 'amount' in update ? update.amount : order.amount;
        if (order.type === OrderType.StopLoss) {
          payload.slSize = `${size}`;
        } else if (order.type === OrderType.TakeProfit) {
          payload.tpSize = `${size}`;
        }
      }

      const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

      if (data.retMsg === 'order not modified') {
        // do nothing, it's ok
      } else if (data.retMsg !== 'OK') {
        this.emitAndLogError(data.retMsg);
      }
    }

    this.scheduleDegradedPrivateSnapshotRefresh();
    return updatedOrderIds;
  };

  cancelOrders = async (orders: Order[]) => {
    // Drop virtual TP/SL display rows — their IDs are synthetic
    // (`{parentId}__stop_loss` / `__take_profit`) and Bybit's cancel API
    // returns "order not exists or too late to cancel" for them. The bracket
    // fields hang off the parent limit order, so the parent's cancellation
    // already drops them. Without this filter, batch cancels containing
    // bracket-attached limits throw entirely on UNIFIED accounts.
    const filteredOrders = orders.filter((o) => !o.id.endsWith('__stop_loss') && !o.id.endsWith('__take_profit'));
    if (filteredOrders.length === 0) return;

    // orders by category
    const ordersByCategory = filteredOrders.reduce(
      (acc, order) => {
        const category = this.getCategoryForSymbol(order.symbol);
        if (!acc[category]) acc[category] = [];
        acc[category].push(order);
        return acc;
      },
      {} as Record<string, Order[]>,
    );

    await forEachSeries(Object.entries(ordersByCategory), async ([category, orders]) => {
      await this.cancelOrdersHelper(orders, category);
    });

    this.scheduleDegradedPrivateSnapshotRefresh();
  };

  cancelOrdersHelper = async (orders: Order[], category: string) => {
    // group into lots of 10
    const lots = chunk(orders, 10);

    await forEachSeries(lots, async (orders, i) => {
      // TODO proper rate limiting
      if (i > 0) await sleep(1000);

      if (this.accountType === 'UNIFIED') {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDERS, {
          category,
          request: orders.map((o) => ({ symbol: o.symbol, orderId: o.id })),
        });
        if (data.retMsg !== 'OK') {
          this.emitAndLogError(data.retMsg);
          throw new Error(data.retMsg);
        }
        const foundErr = data.retExtInfo?.list.find((l: any) => l.code !== 0);
        if (foundErr) {
          this.emitAndLogError(foundErr.msg);
          throw new Error(foundErr.msg);
        }
      } else {
        const errors: Array<string | undefined | null> = [];
        await forEachSeries(orders, async (order) => {
          const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDER, {
            category,
            symbol: order.symbol,
            orderId: order.id,
          });
          if (data.retMsg !== 'OK') {
            errors.push(data.retMsg);
          }
        });
        if (errors.length > 0) {
          this.emitAndLogError(errors.join(', '));
          throw new Error(errors.join(', '));
        }
      }
    });
  };

  cancelSymbolOrders = async (symbol: string, cancelTpSl = true) => {
    const tpOrSLOrders = cancelTpSl
      ? []
      : this.store.orders.filter((o) => o.symbol === symbol && o.type !== OrderType.Limit);

    const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
      category: this.getCategoryForSymbol(symbol),
      symbol,
    });

    // we need to re-create TP/SL after cancel all
    // before bybit was not canceling them
    if (tpOrSLOrders.length > 0) {
      await this.placeOrders(
        tpOrSLOrders.map((o) => ({
          ...o,
          side: o.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        })),
      );
    }

    if (v(data, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data, 'retMsg'));
    }

    this.scheduleDegradedPrivateSnapshotRefresh();
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    let foundPositions = this.store.positions.filter((p) => p.symbol === symbol);
    if (foundPositions.length === 0) {
      foundPositions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(foundPositions);
    }
    const currentLeverage = foundPositions[0]?.leverage ?? 1;

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (currentLeverage !== leverage) {
      const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
        category: this.getCategoryForSymbol(symbol),
        symbol,
        buyLeverage: `${leverage}`,
        sellLeverage: `${leverage}`,
      });

      if (v(data, 'retMsg') !== 'OK') {
        this.emitAndLogError(v(data, 'retMsg'));
        throw new Error(v(data, 'retMsg'));
      }

      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    }
  };

  changePositionMode = async (hedged: boolean, symbol?: string) => {
    symbol = undefined;
    if (symbol) {
      // linear only supports USDT for changing pos modes
      const category = this.getCategoryForSymbol(symbol);
      await this.changePositionModeHelper(hedged, category, 'USDT');
    } else {
      // linear only supports linear usdt for bulk changing pos modes
      await this.changePositionModeHelper(hedged, 'linear', 'USDT');
    }
  };

  changePositionModeHelper = async (hedged: boolean, category?: string, coin?: string, symbol?: string) => {
    const { data } = await this.xhr.post(ENDPOINTS.SET_POSITION_MODE, {
      category,
      coin,
      symbol,
      mode: hedged ? 3 : 0,
    });

    const positions = await this.fetchPositions();
    this.store.update({ positions });
    if (symbol) {
      const symbolPos = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(symbolPos);
    }

    if (data.retMsg === 'All symbols switched successfully.') {
    } else if (
      data.retMsg === 'Partial symbols switched successfully, excluding symbols with open orders or positions.'
    ) {
      this.emitAndLogError(data.retMsg);
    } else {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required when changing margin type');
      throw new Error('Symbol is required when changing margin type');
    }

    let foundPositions = this.store.positions.filter((p) => p.symbol === symbol);
    if (foundPositions.length === 0) {
      foundPositions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(foundPositions);
    }

    const leverage = foundPositions[0]?.leverage || 1;

    const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, {
      category: this.getCategoryForSymbol(symbol),
      symbol,
      tradeMode: isolated ? 1 : 0,
      buyLeverage: `${leverage}`,
      sellLeverage: `${leverage}`,
    });

    if (v(data, 'retMsg') !== 'OK' || +v(data, 'retCode') !== 0) {
      const errorMsg = v(data, 'retMsg') || 'Failed to switch margin type';
      this.emitAndLogError(errorMsg);
      throw new Error(errorMsg);
    }

    // Keep local cache in sync; mapped positions currently read this value.
    this.marginType = isolated ? MarginType.Isolated : MarginType.Cross;

    const positions = await this.fetchPositions([symbol]);
    this.store.addOrUpdatePositions(positions);
  };

  mapPosition(p: Record<string, any>) {
    const entryPrice = parseFloat(v(p, 'avgPrice') || v(p, 'entryPrice') || 0);
    let contracts = parseFloat(p.size ?? 0);
    const side =
      p.positionIdx === 1
        ? PositionSide.Long
        : p.positionIdx === 2
          ? PositionSide.Short
          : POSITION_SIDE[p.side] || PositionSide.Long;

    let unrealizedPnl = parseFloat(v(p, 'unrealisedPnl') || 0);
    const market = this.store.marketsBySymbol[p.symbol];
    const contractsNative = contracts;
    let notional = parseFloat(v(p, 'positionValue') || 0);
    if (market?.isInverse) {
      const ticker = this.store.tickersBySymbol[p.symbol];
      const referencePrice = ticker?.last || ticker?.mark || entryPrice;
      if (referencePrice) {
        contracts = adjust(contracts / referencePrice, 0.0000001);
      }
      notional = contracts;
    } else if (!p.symbol.endsWith('USDT')) {
      const ticker = this.store.tickersBySymbol[p.symbol];
      if (market && market.quote === 'USDC' && ticker) {
        const direction = side === PositionSide.Long ? 1 : -1;
        unrealizedPnl = direction * (ticker.mark * contracts - entryPrice * contracts);
      }
    }

    const positionMode =
      p.positionIdx === undefined ? undefined : p.positionIdx === 0 ? PositionMode.OneWay : PositionMode.Hedged;
    // const marginType =
    //   p.tradeMode === undefined ? undefined : p.tradeMode === 1 ? MarginType.Isolated : MarginType.Cross;

    const marginType = this.marginType;
    const positionIM = parseFloat(v(p, 'positionIM') || 0);
    const position: Position = {
      id: createPositionId(p.symbol, side, positionMode),
      symbol: p.symbol,
      side,
      entryPrice,
      notional,
      leverage: parseFloat(p.leverage),
      unrealizedPnl,
      realizedPnl: parseFloat(v(p, 'curRealisedPnl') || 0),
      contracts,
      liquidationPrice: parseFloat(v(p, 'liqPrice') || 0),
      marginSize: positionIM > 0 ? positionIM : undefined,
      positionMode,
      marginType,
      extra: {
        contractsNative,
      },
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    return position;
  }

  mapOrder(o: Record<string, any>) {
    const stopOrderType = v(o, 'stopOrderType');
    const isStop = Boolean(stopOrderType && stopOrderType !== 'UNKNOWN');

    const triggerPrice = v(o, 'triggerPrice');
    const oPrice = isStop ? triggerPrice || o.price : o.price;
    const oType = isStop ? stopOrderType : v(o, 'orderType');

    const reduceOnly = v(o, 'reduceOnly') === true || v(o, 'reduceOnly') === 'true';
    const closeOnTrigger = v(o, 'closeOnTrigger') === true || v(o, 'closeOnTrigger') === 'true';
    const tpslMode = v(o, 'tpslMode');
    const isPositionTpslMode = isBybitPositionTpslMode(tpslMode);
    const isOpeningTriggerOrder =
      isStop &&
      !reduceOnly &&
      !closeOnTrigger &&
      !isPositionTpslMode &&
      (parseFloat(triggerPrice ?? '0') > 0 || v(o, 'createType') === 'CreateByStopOrder');
    const type = isOpeningTriggerOrder ? OrderType.Conditional : ORDER_TYPE[oType];
    const timeInForce = ORDER_TIME_IN_FORCE[o.timeInForce];

    let remaining = o.leavesQty ? parseFloat(o.leavesQty) : subtract(o.qty ?? 0, v(o, 'cumExecQty') ?? 0);
    const remainingNative = remaining;
    const price = parseFloat(oPrice);
    const market = this.store.marketsBySymbol[o.symbol];
    if (market?.isInverse) {
      remaining = adjust(remaining / price, 0.0000001);
    }

    const orders: Order[] = [
      {
        id: o.orderId,
        status: ORDER_STATUS[v(o, 'orderStatus')],
        symbol: o.symbol,
        type,
        side: ORDER_SIDE[o.side],
        remaining,
        extra: {
          remainingNative,
        },
        price,
        amount: (oType === 'StopLoss' || oType === 'TakeProfit') && tpslMode === 'Full' ? 0 : parseFloat(o.qty ?? 0),
        filled: parseFloat(v(o, 'cumExecQty') ?? 0),
        reduceOnly,
        close: closeOnTrigger,
        info: o,
        timeInForce,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        ...(type === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerBy')] }),
        ...(type === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerBy')] }),
      },
    ];

    const sl = parseFloat(v(o, 'stopLoss'));
    const tp = parseFloat(v(o, 'takeProfit'));

    const inverseSide = orders[0].side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;

    if (sl > 0) {
      orders.push({
        ...orders[0],
        id: `${o.orderId}__stop_loss`,
        type: OrderType.StopLoss,
        side: inverseSide,
        price: sl,
        filled: 0,
        remaining: orders[0].amount,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'slTriggerBy')],
      });
    }

    if (tp > 0) {
      orders.push({
        ...orders[0],
        id: `${o.orderId}__take_profit`,
        type: OrderType.TakeProfit,
        side: inverseSide,
        price: tp,
        filled: 0,
        remaining: orders[0].amount,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'tpTriggerBy')],
      });
    }

    return orders;
  }

  private fetchPositionMode = async (symbol: string): Promise<PositionMode> => {
    const foundPosition = this.store.positions.find((p) => p.symbol === symbol);
    if (foundPosition?.positionMode) return foundPosition.positionMode;

    const { data } = await this.xhr.get(ENDPOINTS.POSITIONS, {
      params: {
        category: this.getCategoryForSymbol(symbol),
        symbol,
      },
    });

    const responseMeta = getBybitResponseMeta(data);
    const positionsList = Array.isArray(data?.result?.list) ? data.result.list : [];

    if (!responseMeta.isSuccess && positionsList.length === 0) {
      const fallbackMessage =
        responseMeta.retMsg ||
        (responseMeta.retCode !== undefined
          ? `Bybit positions request failed with code ${responseMeta.retCode}`
          : 'Bybit positions request returned malformed status payload');

      this.emitAndLogError(fallbackMessage, 'Bybit fetchPositionMode failed', {
        endpoint: ENDPOINTS.POSITIONS,
        symbol,
        retCode: responseMeta.retCode,
        retMsg: responseMeta.retMsg,
        hasExplicitStatus: responseMeta.hasExplicitStatus,
      });
      return PositionMode.OneWay;
    }

    const formattedData = positionsList.map(this.mapPosition.bind(this));
    this.store.addOrUpdatePositions(formattedData);
    return formattedData?.length > 0 ? formattedData[0].positionMode : PositionMode.OneWay;
  };

  private getOrderPositionIdx = async (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'symbol'>) => {
    const positionMode = await this.fetchPositionMode(opts.symbol);
    if (positionMode === PositionMode.OneWay) return 0;

    let positionIdx = opts.side === OrderSide.Buy ? 1 : 2;
    if (opts.reduceOnly) positionIdx = positionIdx === 1 ? 2 : 1;

    return positionIdx;
  };

  private getStopOrderPositionIdx = async (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'symbol'>) => {
    const positionIdx = await this.getOrderPositionIdx(omit(opts, ['reduceOnly']));
    return { 0: 0, 1: 2, 2: 1 }[positionIdx];
  };
}
