import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';
import type { ExchangeName } from '../../../types';
import type { BybitPlaceOrderOpts } from '../../../exchanges/bybit/bybit.order-params';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
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
  OrderStatus,
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
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { mergeUpdatesUsingLastUpdate } from '../../../utils/mergeUpdatesUsingLastUpdate';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, divide, subtract } from '../../../utils/safe-math';
import { isUnknownOutcome } from '../../../utils/unknownSendOutcome';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './bybit.api';
import { fetchBybitIncomeHistory, fetchBybitIncomeHistoryRange, type BybitIncomeContext } from './bybit.income';
import { mapBybitClosedPnl } from '../../../exchanges/bybit/bybit.closed-pnl';
import { getBybitOrderExecutionParams, hasBybitSingleOrderParams } from '../../../exchanges/bybit/bybit.order-params';
import {
  BYBIT_RATE_LIMIT_TABLE as BYBITINVERSE_RATE_LIMIT_TABLE,
  BybitAccountType as BybitInverseAccountType,
  BybitProductType as BybitInverseProductType,
  BybitVipLevel as BybitInverseVipLevel,
} from './bybit.rateLimits';
import { BYBIT_BRAZIL_SITE_ID } from '../../../exchanges/bybit/bybit.site';
import {
  BybitApiKeyInfo as BybitInverseApiKeyInfo,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TIME_IN_FORCE,
  ORDER_TIME_IN_FORCE_OUTBOUND,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE,
} from '../../../exchanges/bybit/bybit.types';
import { BybitInversePrivateWebsocket } from './bybitinverse.ws-private';
import { BybitInversePublicWebsocket } from './bybitinverse.ws-public';
import { bybitFeeOverlay } from './bybit.fee-overlay';

const REINIT_RETRY_MS = 15_000;
const REINIT_RETRY_MAX_MS = 5 * 60_000;

const MAX_CLOSED_PNL_PAGES = 20;

const INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1_000;
const BYBIT_POSITION_TPSL_MODES = new Set(['Full', 'Partial']);
const RECENTLY_PLACED_ORDER_GRACE_MS = 8_000;

const isBybitPositionTpslMode = (mode: unknown) => typeof mode === 'string' && BYBIT_POSITION_TPSL_MODES.has(mode);

const isBybitOrderAlreadyGoneCancelMessage = (message: unknown) => {
  const normalized = `${message || ''}`.toLowerCase();
  return normalized.includes('order not exists') || normalized.includes('too late to cancel');
};

const isBybitOrderNotModifiedMessage = (message: unknown) => `${message || ''}`.toLowerCase().includes('not modified');

const getBybitTriggerBy = (opts: PlaceOrderOpts) => {
  if (opts.type === OrderType.Conditional) {
    if (opts.triggerType === OrderStopTriggerType.MarkPrice) return 'MarkPrice';
    if (opts.triggerType === OrderStopTriggerType.IndexPrice) return 'IndexPrice';
    return 'LastPrice';
  }
  if (opts.type === OrderType.StopLoss) {
    return opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'LastPrice' : 'MarkPrice';
  }
  if (opts.type === OrderType.TakeProfit) {
    return opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MarkPrice' : 'LastPrice';
  }
  return undefined;
};

const getBybitTriggerDirectionPrice = (opts: PlaceOrderOpts, ticker: Ticker) => {
  if (opts.type === OrderType.Conditional) {
    if (opts.triggerType === OrderStopTriggerType.MarkPrice) return ticker.mark;
    if (opts.triggerType === OrderStopTriggerType.IndexPrice) return ticker.index;
    return ticker.last;
  }
  if (opts.type === OrderType.StopLoss) {
    return opts.slTriggerType === OrderStopTriggerType.LastPrice ? ticker.last : ticker.mark;
  }
  if (opts.type === OrderType.TakeProfit) {
    return opts.tpTriggerType === OrderStopTriggerType.LastPrice ? ticker.last : ticker.mark;
  }
  return ticker.last;
};

export class BybitInverseExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Bybit inverse', /^[A-Za-z0-9_-]{1,36}$/);

  xhr: Axios;
  unlimitedXHR: Axios;

  // Only need inverse WebSocket for BybitInverse exchange
  publicInverseWebsocket: BybitInversePublicWebsocket;
  privateWebsocket: BybitInversePrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // Bybit no longer offers Classic accounts. Start in UTA 2.0 Pro mode so
  // account validation never probes the retired CONTRACT wallet API first.
  private unifiedMarginStatus: number = 6;
  private marginType: MarginType = MarginType.Cross;

  private needsReinit = false;
  private lastReinitAt = 0;
  private reinitRetryMs = REINIT_RETRY_MS;
  // A thrown request (an unavailable or timed-out remote signer) must trigger the
  // full refetch exactly like a venue error does.
  private reinitOnThrow = <T>(request: Promise<T>): Promise<T> =>
    request.catch((error: unknown) => {
      this.needsReinit = true;
      throw error;
    });

  positionMode: PositionMode | null = null;

  lastWebsocketPositionsUpdate: number | undefined;

  private apiKeyInfo: BybitInverseApiKeyInfo | null = null;

  private batchLimit: number = 10;
  private readonly EXECUTION_QUERY_MAX_RANGE_MS = 7 * 24 * 60 * 60 * 1000 - 1;
  private readonly DEFAULT_EXECUTIONS_LOOKBACK_MS = 30 * 24 * 60 * 60 * 1000;

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

  get supportedCategories() {
    // Only support inverse category for BybitInverse exchange
    return ['inverse'];
  }

  getSettleCoinsForCategory(category: string) {
    if (category === 'linear') {
      if (this.accountType === 'UNIFIED') return ['USDT', 'USDC'];
      return ['USDT'];
    } else if (category === 'inverse') return [undefined];
    this.emitAndLogError(`Unknown category ${category}`);
    throw new Error(`Unknown category ${category}`);
  }

  getCategoryForSymbol = (symbol: string) => {
    // For BybitInverse, always return 'inverse' for valid markets
    const market = this.store.markets.find((m) => m.symbol === symbol);
    if (!market) {
      // dont spam console if markets not loaded
      if (this.store.markets.length > 0) {
        console.warn(`Market ${symbol} not found`);
      }
      // Default to inverse for BybitInverse exchange
      return 'inverse';
    }
    // All markets in BybitInverse should be inverse (USD quoted)
    return 'inverse';
  };

  getTickerSymbolsForPublicStream() {
    const inverseSymbols = this.store.markets
      .filter((market) => market.quote !== 'USDT' && market.quote !== 'USDC')
      .map((market) => market.symbol);

    return this.getTickerSymbolsForSubscription(inverseSymbols);
  }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);

    // Initialize only inverse WebSocket
    this.publicInverseWebsocket = new BybitInversePublicWebsocket(this);
    this.privateWebsocket = new BybitInversePrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicInverseWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  private recentlyPlacedOrders = new Map<string, { placedAt: number; order: Order }>();

  private shouldUseLocalRestOrderLifecycleFallback = () => {
    return this.options.bybitSiteId === BYBIT_BRAZIL_SITE_ID;
  };

  /** First sighting via private WS or an explicit cancel — stop protecting the id. */
  markOrderSeen = (orderId: string) => {
    this.recentlyPlacedOrders.delete(orderId);
  };

  // Bybit sizes inverse orders in USD contracts; the store holds `remaining` in coin and the
  // contracts in `extra.remainingNative`, or the chart prices USD as coin (qty x price). Every
  // market here is inverse, so this must not wait on `marketsBySymbol` having loaded.
  private remainingInStoreUnits = (remainingNative: number, price: number) => {
    const remaining = price > 0 ? Math.round((remainingNative / price) * 1e8) / 1e8 : remainingNative;
    return { remaining, extra: { remainingNative, remainingNotional: remainingNative } };
  };

  private recordRecentlyPlacedOrderFromOpts = (orderId: string, opts: PlaceOrderOpts, payload: Record<string, any>) => {
    const qty = parseFloat(payload.qty as string) || opts.amount || 0;
    const price = parseFloat((payload.price ?? payload.triggerPrice) as string) || opts.price || opts.triggerPrice || 0;

    this.recordRecentlyPlacedOrder({
      id: orderId,
      status: OrderStatus.Open,
      symbol: opts.symbol,
      type: opts.type,
      side: opts.side,
      price,
      amount: qty,
      filled: 0,
      ...this.remainingInStoreUnits(qty, price),
      reduceOnly: (opts.type === OrderType.Conditional ? false : opts.reduceOnly) || false,
      timeInForce: opts.timeInForce || OrderTimeInForce.GoodTillCancel,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    });
  };

  private recordRecentlyPlacedOrder = (order: Order) => {
    if (order.type === OrderType.Market) {
      return;
    }

    this.recentlyPlacedOrders.set(order.id, { placedAt: virtualClock.getCurrentTimeMs(), order });

    if (this.shouldUseLocalRestOrderLifecycleFallback()) {
      this.store.addOrUpdateOrders([order]);
    }
  };

  private retireCancelledOrders = (orders: Array<Pick<Order, 'id' | 'symbol'>>) => {
    if (orders.length === 0) return;

    for (const order of orders) {
      this.markOrderSeen(order.id);
    }

    if (this.shouldUseLocalRestOrderLifecycleFallback()) {
      this.store.removeOrders(orders.map(({ id }) => ({ id })));
    }
  };

  private applyLocalOrderUpdateAfterRestWrite = (order: Order, update: Partial<Order>) => {
    if (!this.shouldUseLocalRestOrderLifecycleFallback()) return;

    const remainingNative =
      typeof update.amount === 'number'
        ? Math.max(0, update.amount - (order.filled || 0))
        : (order.extra?.remainingNative ?? order.remaining);
    const nextOrder: Order = {
      ...order,
      ...update,
      ...this.remainingInStoreUnits(remainingNative, update.price ?? order.price),
      extra: { ...order.extra, remainingNative, remainingNotional: remainingNative },
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    this.store.addOrUpdateOrders([nextOrder]);
  };

  private preserveRecentlyPlacedOrders = (fetched: Order[]): Order[] => {
    if (this.recentlyPlacedOrders.size === 0) return fetched;

    const now = virtualClock.getCurrentTimeMs();
    const fetchedIds = new Set(fetched.map((o) => o.id));
    const preserved: Order[] = [];

    for (const [id, entry] of this.recentlyPlacedOrders) {
      if (fetchedIds.has(id)) {
        this.recentlyPlacedOrders.delete(id);
        continue;
      }
      if (now - entry.placedAt > RECENTLY_PLACED_ORDER_GRACE_MS) {
        this.recentlyPlacedOrders.delete(id);
        continue;
      }
      preserved.push(this.store.orders.find((o) => o.id === id) ?? entry.order);
    }

    return preserved.length > 0 ? [...fetched, ...preserved] : fetched;
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

      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
    await virtualClock.start();

    // Reset batch limit on exchange start
    this.batchLimit = 10;

    const publicDataOnly = this.isPublicDataOnly;

    if (this.needsAccountMarketOverlay) {
      await this.loadAccountMarketOverlay(async () =>
        bybitFeeOverlay((await Promise.all(this.supportedCategories.map((c) => this.fetchFees(c)))).flat()),
      );
      if (this.isDisposed) return;
    }

    const useExternalPublicData = await this.initExternalPublicData();

    // Margin mode and unified-account status are private, and every position
    // is stamped with them. They used to be fetched inside the public-data
    // branch below, so an account sourcing public data from elsewhere kept
    // reporting cross margin regardless of its real setting.
    // Handled where it is created: a rejected key fails this before the market fetch
    // below finishes, and a rejection with no handler yet ends a Node process.
    const privateProbe = publicDataOnly
      ? null
      : this.fetchMarginAccountInfos().then(
          () => undefined,
          (err: any) => String(err?.message || err) || 'request failed',
        );

    // Fetch API key info asynchronously without blocking startup
    if (!publicDataOnly) {
      this.fetchAndCacheApiKeyInfo().catch(() => {
        // Already handled in fetchAndCacheApiKeyInfo
      });
    }

    if (!useExternalPublicData) {
      const [_, rawTickers] = await Promise.all([
        await this.fetchMarkets().then((markets) => {
          if (this.isDisposed) return [];
          this.log(`Loaded ${markets.length} BybitInverse markets`);
          this.store.update({
            markets,
            loaded: { ...this.store.loaded, markets: true },
          });

          // start public websocket stream for inverse markets
          this.publicInverseWebsocket.connectAndSubscribe();

          return markets;
        }),
        await this.fetchTickersWithoutParse(),
      ]);

      const tickers = this.parseTickers(rawTickers);

      if (this.isDisposed) return;

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });
    }

    // Awaited here rather than inside the branch above: it must run on both
    // public-data paths, and it is what stamps margin mode onto positions.
    const privateProbeFailure = privateProbe ? await privateProbe : undefined;

    // A rejected key comes back as HTTP 200 with a non-OK retMsg, so the probe is the
    // only thing that knows. `authFailureStreak` cannot be used: markets and tickers
    // are signed too, and their success calls recordAuthSuccess() after the probe.
    if (privateProbeFailure) {
      this.log(
        `BybitInverse market data is ready, but the private API was rejected (${privateProbeFailure}) — trading is unavailable`,
        LogSeverity.Warning,
      );
    } else {
      this.log(`Ready to trade on BybitInverse`);
    }

    // start ticking live data
    // balance, tickers, positions
    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
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
        this.fetchPositions().then((fetchedPositions) => {
          if (this.isDisposed) return [];
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
      // Thrown, not returned: this is the only signal for the readiness log below
      // that cannot be reset by a later request. Emitting first is required —
      // see exchanges/CLAUDE.md, "Pre-flight validation throws must emit first".
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }

    this.marginType = data?.result?.marginMode === 'ISOLATED_MARGIN' ? MarginType.Isolated : MarginType.Cross;
    this.unifiedMarginStatus = data?.result?.unifiedMarginStatus;
  };

  tick = async () => {
    if (!this.isDisposed) {
      // Cleared BEFORE the refetch so a failure inside can set it again; backed off so
      // a persistent failure (a key missing a permission) is not refetched every tick.
      if (this.needsReinit && Date.now() - this.lastReinitAt >= this.reinitRetryMs) {
        this.needsReinit = false;
        this.lastReinitAt = Date.now();
        await this.fetchOrdersPositionsAndBalance();
        this.reinitRetryMs = this.needsReinit
          ? Math.min(this.reinitRetryMs * 2, REINIT_RETRY_MAX_MS)
          : REINIT_RETRY_MS;
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
  };

  fetchApiKeyInfo = async (): Promise<BybitInverseApiKeyInfo> => {
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
          bybitinverseApiKeyInfo: this.apiKeyInfo,
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
    const rateLimitEntry = BYBITINVERSE_RATE_LIMIT_TABLE.find(
      (entry) => entry.level === (this.apiKeyInfo!.vipLevel as BybitInverseVipLevel),
    );
    if (!rateLimitEntry) {
      this.log(`No rate limit entry found for VIP level: ${this.apiKeyInfo.vipLevel}`);
      return;
    }

    // Get the futures rate limit based on account type
    const accountType =
      this.accountType === 'UNIFIED' ? BybitInverseAccountType.UTAPro : BybitInverseAccountType.ClassicAndUTA;
    const futuresRateLimit = rateLimitEntry[BybitInverseProductType.Futures][accountType];

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
    const orders = await mapSeries(this.supportedCategories, async (category) => {
      const settleCoins = this.getSettleCoinsForCategory(category);
      const orders = await mapSeries(settleCoins, async (settleCoin) => {
        return this.fetchOrdersHelper(category, settleCoin);
      });
      return orders.flat();
    });
    return this.preserveRecentlyPlacedOrders(orders.flat());
  };

  fetchOrdersHelper = async (category: string, settleCoin?: string) => {
    const recursiveFetch = async (
      cursor: string = '',
      orders: Array<Record<string, any>> = [],
    ): Promise<Array<Record<string, any>>> => {
      const { data } = await this.reinitOnThrow(
        this.xhr.get(ENDPOINTS.UNFILLED_ORDERS, {
          params: {
            category,
            settleCoin,
            limit: 50,
            cursor,
          },
        }),
      );

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
      const positions = await mapSeries(this.supportedCategories, async (category) => {
        const settleCoins = this.getSettleCoinsForCategory(category);
        const positions = await mapSeries(settleCoins, async (settleCoin) => {
          return this.fetchPositionsHelper(undefined, category, settleCoin);
        });
        return positions.flat();
      });
      return positions.flat();
    }
  };

  fetchPositionsHelper = async (symbols?: string[], category?: string, settleCoin?: string) => {
    const fetchAllPages = async (
      params: Record<string, unknown>,
      failClosed: boolean,
      cursor = '',
      accumulated: Array<Record<string, any>> = [],
    ): Promise<Array<Record<string, any>>> => {
      const { data } = await this.reinitOnThrow(
        this.xhr.get(ENDPOINTS.POSITIONS, {
          params: { ...params, limit: 200, cursor },
        }),
      );
      if (v(data, 'retMsg') !== 'OK') {
        this.emitAndLogError(v(data, 'retMsg'));
        if (failClosed) throw new Error(v(data, 'retMsg'));
        this.needsReinit = true;
        return [];
      }

      const page = Array.isArray(data?.result?.list) ? data.result.list : [];
      const positions = [...accumulated, ...page];
      const nextPageCursor = data?.result?.nextPageCursor;
      if (nextPageCursor && nextPageCursor !== cursor) {
        return fetchAllPages(params, failClosed, nextPageCursor, positions);
      }
      return positions;
    };

    if (symbols && symbols.length > 0) {
      const positions: Position[][] = await mapSeries(symbols, async (symbol) => {
        const bybitPositions = await fetchAllPages({ symbol, category: this.getCategoryForSymbol(symbol) }, true);
        return bybitPositions.map(this.mapPosition.bind(this));
      });
      return positions.flat();
    } else {
      const bybitPositions = await fetchAllPages({ category, settleCoin }, false);
      return bybitPositions.map(this.mapPosition.bind(this));
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
    const foundTickers = await Promise.all(this.supportedCategories.map(this.fetchTickersRawHelper.bind(this)));
    const newTickers = [];
    for (const tickerSet of foundTickers) {
      newTickers.push(...tickerSet);
    }
    if (newTickers.some((t) => t === null)) {
      this.emitAndLogError('Failed to fetch tickers');
      return null;
    }
    return newTickers;
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

    // Indexed for-loop + push instead of the O(N²) `reduce + [...acc, t]`
    // pattern.
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
    // try {
    //   const { data } = await this.dataXHR.get('/markets?exchange=bybit', { timeout: 2000 });
    //   if (Array.isArray(data) && data.length > 0) {
    //     return data as Market[];
    //   }
    // } catch (e) {}
    const markets = await Promise.all(this.supportedCategories.map(this.fetchMarketsHelper.bind(this)));
    const newMarkets = markets.flat();
    if (newMarkets.some((m) => m === null)) {
      this.emitAndLogError('Failed to fetch markets');
      return this.store.markets;
    }
    return newMarkets as Market[];
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

    const { data } = await this.xhr.get(ENDPOINTS.MARKETS, {
      params: { category, limit: 1000 },
    });

    const { data: data2 } = await this.xhr.get(ENDPOINTS.MARKETS, {
      params: { category, limit: 1000, status: 'PreLaunch' },
    });

    if (v(data, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data, 'retMsg'));
      return [null];
    }

    if (v(data2, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data2, 'retMsg'));
      return [null];
    }

    const markets: Market[] = [...data.result.list, ...data2.result.list]
      // .filter((market: Record<string, any>) => market.quoteCoin === 'USDT')
      .map((market: Record<string, any>) => {
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
          isInverse: category === 'inverse',
          // isReadOnly: category === 'inverse',
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

  /** Inverse contracts, so `category: 'inverse'`. See `bybit.exchange.ts` for why this
   *  no longer reads Tealstreet's own dead cache. */
  fetchClosedPnl = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const to = opts.to ?? virtualClock.getCurrentTimeMs();
    const from = opts.from ?? to - this.DEFAULT_EXECUTIONS_LOOKBACK_MS;
    const entries: ClosedPnlEntry[] = [];

    try {
      for (const window of this.getExecutionQueryWindows(from, to)) {
        let cursor: string | undefined;
        let page = 0;

        do {
          const { data } = await this.xhr.get(ENDPOINTS.USER_CLOSED_PNL, {
            params: omitUndefined({
              category: 'inverse',
              symbol: opts.symbol,
              startTime: window.startTime,
              endTime: window.endTime,
              limit: 100,
              cursor,
            }),
          });

          if (data.retMsg !== 'OK' && +data.retCode !== 0) {
            this.log(`Failed to fetch closed PnL: ${data.retMsg}`, LogSeverity.Error);
            break;
          }

          const rows = (data.result?.list || []) as Array<Record<string, any>>;
          for (const row of rows) {
            const entry = mapBybitClosedPnl(row, opts.accountName || '', true);
            if (entry) entries.push(entry);
          }

          cursor = data.result?.nextPageCursor || undefined;
          page += 1;

          // Results are newest-first, so a cap that bites drops the OLDEST rows of the
          // window — a hole in the middle of the history rather than a short tail.
          if (cursor && page >= MAX_CLOSED_PNL_PAGES) {
            this.log(
              `Closed-PnL paging hit the ${MAX_CLOSED_PNL_PAGES}-page cap for ${opts.symbol}; older rows in this window were not fetched`,
              LogSeverity.Warning,
            );
          }
        } while (cursor && page < MAX_CLOSED_PNL_PAGES);
      }

      return entries.sort((a, b) => b.closedTime - a.closedTime);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.retMsg || err?.message);
      return [];
    }
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

    const { data } = await this.xhr.get(ENDPOINTS.KLINE, { params });

    const candles: Candle[] = orderBy(
      data?.result?.list?.map?.(([open_time, open, high, low, close, volume]: string[]) => {
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

  private fetchExecutionsFallbackFromOrderHistory = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    const { symbol, orderId, from, to, limit = 50 } = opts;
    if (!symbol) return [];

    const fallbackTo = to ?? virtualClock.getCurrentTimeMs();
    const fallbackFrom = from ?? (to === undefined ? fallbackTo - this.DEFAULT_EXECUTIONS_LOOKBACK_MS : undefined);
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

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 50 } = opts;

    if (!symbol) {
      // BybitInverse requires symbol for execution queries
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
      // Keep parity with linear bybit adapter to avoid unbounded cursor walks.
      if (depth >= MAX_RECURSION_DEPTH) {
        this.log(`fetchHistoricalFills reached max recursion depth of ${MAX_RECURSION_DEPTH}`, LogSeverity.Warning);
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
            id: exec.execId ? String(exec.execId) : undefined,
            timestamp,
            symbol: exec.symbol || symbol,
            orderId: exec.orderId,
            price,
            qty,
            side: isBuy ? OrderSide.Buy : OrderSide.Sell,
            commission: Math.abs(parseFloat(exec.execFee ?? exec.fee ?? '0')),
            commissionAsset: exec.feeCurrency || exec.feeCoin || exec.quoteCoin,
            maker: typeof exec.isMaker === 'boolean' ? exec.isMaker : undefined,
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
      const fallbackWarning = `fetchHistoricalFills primary path produced no valid fills for ${symbol}; falling back to ${ENDPOINTS.USER_ORDERS} (rawRows=${primaryRowsCount}, windows=${queryWindows.length}, limit=${limit}, hasOrderId=${Boolean(orderId)})`;
      this.log(fallbackWarning, LogSeverity.Warning);
      if (typeof console !== 'undefined' && typeof console.warn === 'function') {
        console.warn(`[${this.name}] ${fallbackWarning}`);
      }
      const fallbackExecutions = await this.fetchExecutionsFallbackFromOrderHistory(opts);
      if (fallbackExecutions.length === 0) {
        this.log(`fetchHistoricalFills fallback returned no rows for ${symbol}`, LogSeverity.Warning);
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
   * The transaction log, which is what fill-symbol ENUMERATION reads.
   *
   * Without these two methods this class inherited `BaseExchange`'s branded stub,
   * which is a function returning `[]`. `canEnumerateFillSymbols` duck-types on
   * `typeof === 'function'`, so inverse accounts were reported as enumerable,
   * the enumerator asked the stub, got no symbols, and collection completed in
   * apparent health having fetched nothing — no stored fills, and so no fill
   * arrows on an inverse chart while the USDT pair worked fine.
   *
   * `accountType` is hardcoded UNIFIED, deliberately, and must NOT be wired to this
   * class's `accountType` getter. The transaction log only serves UNIFIED accounts,
   * so sending the CONTRACT that getter returns for a classic account earns a
   * non-zero retCode, which this path turns into an empty list — indistinguishable
   * from "you traded nothing". Linear hardcodes it for the same reason.
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> =>
    fetchBybitIncomeHistory(this.incomeContext, opts);

  fetchIncomeHistoryRange = async (startTime: number, endTime: number, incomeType?: string): Promise<IncomeRecord[]> =>
    fetchBybitIncomeHistoryRange(this.incomeContext, startTime, endTime, incomeType);

  private get incomeContext(): BybitIncomeContext {
    return { xhr: this.xhr, log: this.log, category: 'inverse', accountType: 'UNIFIED' };
  }

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // BybitInverse only supports inverse futures
    return this.publicInverseWebsocket.listenOHLCV(opts, callback);
  };

  listenOHLCVs = (
    opts: OHLCVSOptions,
    callback: (candle: Candle & { symbol: string; interval: Timeframe }) => void,
  ) => {
    // BybitInverse only supports inverse futures, so all symbols are inverse
    this.publicInverseWebsocket.listenOHLCVs(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    // BybitInverse only supports inverse futures
    return this.publicInverseWebsocket.listenOrderBook(symbol, callback);
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

  private getOrderExecutionParams = (opts: PlaceOrderOpts) => {
    try {
      return getBybitOrderExecutionParams(opts, this.getCategoryForSymbol(opts.symbol));
    } catch (error: any) {
      const message = error?.message || `${error}`;
      this.emitAndLogError(message);
      throw error;
    }
  };

  placeOrder = async (opts: BybitPlaceOrderOpts) => {
    const clientOrderId = this.resolveCallerClientOrderId(opts);
    const category = this.getCategoryForSymbol(opts.symbol);
    const orderExecutionParams = this.getOrderExecutionParams(opts);

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

    let amount: number;

    // Handle amount based on market type
    if (market.isInverse) {
      // For Bybit inverse perpetuals:
      // - Each contract = $1 USD
      // - opts.amount should already be in USD contracts
      // - Just ensure it's an integer
      amount = Math.round(opts.amount);

      // Ensure we meet minimum order size requirement
      const minOrderQty = market.limits.amount.min || 1;
      if (amount < minOrderQty) {
        amount = minOrderQty;
      }

      // Ensure we don't exceed maximum order size
      const maxOrderQty = market.limits.amount.max;
      if (maxOrderQty && amount > maxOrderQty) {
        amount = maxOrderQty;
      }

      this.log(`BybitInverse order: input amount=${opts.amount}, USD contracts=${amount}, symbol=${opts.symbol}`);
    } else {
      // For linear contracts, apply precision adjustment
      amount = adjust(opts.amount, pAmount);
    }

    const price = opts.price ? adjust(opts.price, pPrice) : null;
    const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
    const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;
    const timeInForce = ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
    const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;

    let triggerDirection: number | undefined = undefined;
    if (triggerPrice) {
      const ticker = this.store.tickersBySymbol[opts.symbol]!;
      const priceToCompare = getBybitTriggerDirectionPrice(opts, ticker);
      if (triggerPrice > priceToCompare) {
        triggerDirection = 1;
      } else {
        triggerDirection = 2;
      }
    }

    let type = opts.type;
    if (opts.type === OrderType.Conditional) {
      if (!triggerPrice) {
        const message = `Trigger price is required for conditional Bybit inverse orders (${opts.symbol})`;
        this.emitAndLogError(message);
        throw new Error(message);
      }
      type = price ? OrderType.Limit : OrderType.Market;
    }
    if (triggerPrice && (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)) {
      type = OrderType.Market;
    }

    const req = omitUndefined({
      category,
      symbol: opts.symbol,
      side: inverseObj(ORDER_SIDE)[opts.side],
      orderType: inverseObj(ORDER_TYPE)[type],
      qty: market.isInverse ? `${Math.floor(amount)}` : `${amount}`, // Ensure integer for inverse
      price:
        orderExecutionParams.bboSideType === undefined &&
        (opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price))
          ? `${price}`
          : undefined,
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
      ...orderExecutionParams,
    });

    // For inverse contracts, we work with integer USD values
    const lots = amount > maxSize ? Math.ceil(divide(amount, maxSize)) : 1;
    let rest: number;
    let lotSize: number;

    if (market.isInverse) {
      // For inverse, keep everything as integers
      rest = amount > maxSize ? Math.floor(amount % maxSize) : 0;
      lotSize = Math.floor(divide(subtract(amount, rest), lots));
    } else {
      // For linear contracts, use precision adjustment
      rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;
      lotSize = adjust(divide(subtract(amount, rest), lots), pAmount);
    }

    const payloads = times(lots, (idx) => {
      // We want to remove stopLoss and takeProfit from the rest of the orders
      // because they are already set on the first one
      const payload = idx > 0 ? omit(req, ['stopLoss', 'takeProfit', 'slTriggerBy', 'tpTriggerBy']) : req;

      return { ...payload, qty: `${lotSize}`, ...(clientOrderId !== undefined && { orderLinkId: clientOrderId }) };
    });

    if (rest) payloads.push({ ...req, qty: `${rest}` });
    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);

    const responses = await mapSeries(payloads, async (p) => {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.CREATE_ORDER, p);
        return data;
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.retMsg || err.message);
        // A lost response on an order carrying the caller's client id may hide a live
        // order: throw, so the caller looks it up rather than reading `[]` as "not placed".
        if (payloads.length === 1 && clientOrderId !== undefined && isUnknownOutcome(err)) throw err;
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

    return fullfilled.map((resp, index) => {
      const orderId = resp.result.orderId;
      const payload = payloads[index];
      if (orderId && payload) {
        this.recordRecentlyPlacedOrderFromOpts(orderId, opts, payload);
      }
      return orderId;
    });
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

    if (isBybitOrderNotModifiedMessage(data.retMsg)) {
      return [];
    }

    if (data.retMsg !== 'OK') {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }

    // SET_TRADING_STOP endpoint doesn't return an orderId
    // Return an empty array as the order is directly applied to the position
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

    if (isBybitOrderNotModifiedMessage(data.retMsg)) {
      return [];
    }

    if (data.retMsg !== 'OK') {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }

    // SET_TRADING_STOP endpoint doesn't return an orderId
    // Return an empty array as the order is directly applied to the position
    return [];
  };

  placeOrders = async (orders: BybitPlaceOrderOpts[]): Promise<string[]> => {
    if (orders.length === 0) {
      return [];
    }

    this.assertCallerClientOrderIds(orders);
    for (const order of orders) {
      this.getOrderExecutionParams(order);
    }

    // Separate orders that need special handling
    const specialOrders: PlaceOrderOpts[] = [];
    const regularOrders: PlaceOrderOpts[] = [];

    for (const order of orders) {
      // Check if order needs special handling (stop loss/take profit with existing position or trailing stop)
      if (
        (this._hasFullSizedPositionOppositeSide(order.symbol, order.side, order.amount) &&
          (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit)) ||
        order.type === OrderType.TrailingStopLoss ||
        hasBybitSingleOrderParams(order)
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
    // BybitInverse rate limit: max 10 orders per second, so we limit batches to 10 orders
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
    const batchRequests = await Promise.all(
      orders.map(async (order) => ({
        order,
        request: await this.formatOrderForBatch(order),
      })),
    );

    // Filter out any null requests (orders that couldn't be formatted)
    const validRequests = batchRequests.filter(
      (entry): entry is { order: PlaceOrderOpts; request: Record<string, any> } => entry.request !== null,
    );

    if (validRequests.length === 0) {
      return [];
    }

    const payload = {
      category,
      request: validRequests.map(({ request }) => request),
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
      data.result.list.forEach((result: any, index: number) => {
        const request = validRequests[index];
        if (result.orderId) {
          orderIds.push(result.orderId);
          if (request) {
            this.recordRecentlyPlacedOrderFromOpts(result.orderId, request.order, request.request);
          }
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

      let amount: number;

      // Handle amount based on market type
      if (market.isInverse) {
        // For Bybit inverse perpetuals:
        // - Each contract = $1 USD
        // - opts.amount should already be in USD contracts
        // - Just ensure it's an integer
        amount = Math.round(opts.amount);

        // Ensure we meet minimum order size requirement
        const minOrderQty = market.limits.amount.min || 1;
        if (amount < minOrderQty) {
          amount = minOrderQty;
        }

        // Ensure we don't exceed maximum order size
        const maxOrderQty = market.limits.amount.max;
        if (maxOrderQty && amount > maxOrderQty) {
          amount = maxOrderQty;
        }
      } else {
        // For linear contracts, apply precision adjustment
        amount = adjust(opts.amount, pAmount);
      }
      const price = opts.price ? adjust(opts.price, pPrice) : null;
      const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
      const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;
      const timeInForce = ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
      const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;

      let triggerDirection: number | undefined = undefined;
      if (triggerPrice) {
        const ticker = this.store.tickersBySymbol[opts.symbol]!;
        const priceToCompare = getBybitTriggerDirectionPrice(opts, ticker);
        if (triggerPrice > priceToCompare) {
          triggerDirection = 1;
        } else {
          triggerDirection = 2;
        }
      }

      let type = opts.type;
      if (opts.type === OrderType.Conditional) {
        if (!triggerPrice) {
          throw new Error(`Trigger price is required for conditional Bybit inverse orders (${opts.symbol})`);
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
        qty: market.isInverse ? `${Math.floor(amount)}` : `${amount}`, // Ensure integer for inverse
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
        orderLinkId: opts.clientOrderId,
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
      const orderIds = await this.placeTrailingStopLoss({ ...order, ...update });
      this.applyLocalOrderUpdateAfterRestWrite(order, update);
      return orderIds;
    }

    // Only cancel+replace for *non-virtual* TP/SL. Virtual TP/SL (e.g. limit order with TP) are updated via REPLACE_ORDER on parent below.
    // Use placeStopLossOrTakeProfit (SET_TRADING_STOP), not placeOrder (CREATE_ORDER does not accept orderType TakeProfit/StopLoss).
    // projectedType ensures we re-place with correct TP vs SL based on new price vs mark, avoiding duplicate-order issues with Bybit AMEND/EDIT.
    if (
      !isVirtualSLorTP &&
      (order.type === OrderType.TakeProfit || order.type === OrderType.StopLoss) &&
      'price' in update &&
      isBybitPositionTpslMode(order.info?.tpslMode)
    ) {
      const mark = this.store.tickersBySymbol[order.symbol]?.mark;
      if (mark) {
        const projectedType =
          order.side === OrderSide.Sell
            ? update.price > mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss
            : update.price <= mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss;
        await this.cancelOrders([order]);
        const orderIds = await this.placeStopLossOrTakeProfit({ ...order, ...update, type: projectedType });
        this.applyLocalOrderUpdateAfterRestWrite(order, { ...update, type: projectedType });
        return orderIds;
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
            this.applyLocalOrderUpdateAfterRestWrite(order, update);
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

    // likely placed outside of tealstreet?
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
        this.applyLocalOrderUpdateAfterRestWrite(order, update);
      } else if (data.retMsg === 'order not modified') {
        // do nothing, it's ok
      } else {
        this.emitAndLogError(data.retMsg);
      }
    }

    // If we want to update the stop loss or take profit order
    // we need to do it on the opened position
    if (
      !isConditionalOrder &&
      !isVirtualSLorTP &&
      (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit)
    ) {
      payload = {
        category: this.getCategoryForSymbol(order.symbol),
        symbol: order.symbol,
        positionIdx: await this.getOrderPositionIdx(order),
      };

      if ('price' in update) {
        if (order.type === OrderType.StopLoss) {
          payload.stopLoss = `${update.price}`;
        } else if (order.type === OrderType.TakeProfit) {
          payload.takeProfit = `${update.price}`;
        }
      }

      const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

      if (data.retMsg === 'order not modified') {
        // do nothing, it's ok
      } else if (data.retMsg !== 'OK') {
        this.emitAndLogError(data.retMsg);
      } else {
        this.applyLocalOrderUpdateAfterRestWrite(order, update);
      }
    }

    // TODO: do we really want to be manually updating the store here...
    // const storeOrder = this.store.orders.find((o) => o.id === order.id && o.symbol === order.symbol);
    //
    // if (storeOrder && payload) {
    //   this.store.updateOrder(storeOrder, payload);
    // }

    return updatedOrderIds;
  };

  cancelOrders = async (orders: Order[]) => {
    for (const order of orders) this.markOrderSeen(order.id);

    // Drop virtual TP/SL display rows — see bybit.exchange.ts cancelOrders
    // for the full rationale. Synthetic IDs (`__stop_loss` / `__take_profit`)
    // are not real exchange orders; the parent's cancellation removes them.
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
  };

  cancelOrdersHelper = async (orders: Order[], category: string) => {
    // group into lots of 10
    const lots = chunk(orders, 10);

    await forEachSeries(lots, async (orders, i) => {
      // TODO proper rate limiting
      if (i > 0) await sleep(1000);

      const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDERS, {
        category,
        request: orders.map((o) => ({ symbol: o.symbol, orderId: o.id })),
      });
      if (data.retMsg !== 'OK') {
        if (isBybitOrderAlreadyGoneCancelMessage(data.retMsg)) {
          this.retireCancelledOrders(orders);
          return;
        }
        this.emitAndLogError(data.retMsg);
        throw new Error(data.retMsg);
      }
      const foundErr = data.retExtInfo?.list.find(
        (l: any) => l.code !== 0 && !isBybitOrderAlreadyGoneCancelMessage(l.msg),
      );
      if (foundErr) {
        this.emitAndLogError(foundErr.msg);
        throw new Error(foundErr.msg);
      }

      this.retireCancelledOrders(orders);
    });
  };

  cancelSymbolOrders = async (symbol: string, cancelTpSl = true) => {
    const tpOrSLOrders = cancelTpSl
      ? []
      : this.store.orders.filter((o) => o.symbol === symbol && o.type !== OrderType.Limit);

    const cancelledRows = this.store.orders
      .filter((o) => o.symbol === symbol)
      .map(({ id, symbol: orderSymbol }) => ({ id, symbol: orderSymbol }));

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
      return;
    }

    this.retireCancelledOrders(cancelledRows);
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
    // uncomment to support setting individual symbol position mode, else bybit tries to set all that it can
    // (that don't have open position)
    symbol = undefined;
    if (symbol) {
      const category = this.getCategoryForSymbol(symbol);
      if (category === 'inverse') {
        await this.changePositionModeHelper(hedged, category, undefined, symbol);
      } else {
        // linear only supports USDT for changing pos modes
        await this.changePositionModeHelper(hedged, category, 'USDT');
      }
    } else {
      // linear only supports linear usdt for bulk changing pos modes
      await this.changePositionModeHelper(hedged, 'linear', 'USDT');
    }
  };

  changePositionModeHelper = async (hedged: boolean, category?: string, coin?: string, symbol?: string) => {
    // if (this.store.positions.filter((p) => p.contracts > 0).length > 0) {
    //   this.emitAndLogError('Please close all positions before switching position mode');
    //   throw new Error('Please close all positions before switching position mode');
    // }

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
      // TODO fix hedged
      // this.store.setSetting('isHedged', hedged);
    } else if (
      data.retMsg === 'Partial symbols switched successfully, excluding symbols with open orders or positions.'
    ) {
      this.emitAndLogError(data.retMsg);
    } else {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }
  };

  changeMarginType = async (isolated: boolean, _symbol?: string) => {
    const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_MODE, {
      setMarginMode: isolated ? 'ISOLATED_MARGIN' : 'REGULAR_MARGIN',
    });

    // Success is retCode 0 with no reasons. Do not compare retMsg: this
    // endpoint answers a successful switch with "Request accepted", not "OK".
    const reasons = Array.isArray(data?.result?.reasons) ? data.result.reasons : [];
    if (+v(data, 'retCode') !== 0 || reasons.length > 0) {
      const reasonMessage = reasons
        .map((reason: any) => reason?.reasonMsg)
        .filter(Boolean)
        .join(', ');
      const errorMsg = reasonMessage || v(data, 'retMsg') || 'Failed to switch margin type';
      this.emitAndLogError(errorMsg);
      throw new Error(errorMsg);
    }

    this.marginType = isolated ? MarginType.Isolated : MarginType.Cross;
    const positions = await this.fetchPositions();
    this.store.update({ positions });
  };

  mapPosition(p: Record<string, any>) {
    const entryPrice = parseFloat(v(p, 'avgPrice') || v(p, 'entryPrice') || 0);
    const contractsNative = parseFloat(p.size ?? 0); // USD contracts from API
    let contracts = contractsNative; // Will be converted to BTC for inverse

    const side =
      p.positionIdx === 1
        ? PositionSide.Long
        : p.positionIdx === 2
          ? PositionSide.Short
          : POSITION_SIDE[p.side] || PositionSide.Long;

    let unrealizedPnl = parseFloat(v(p, 'unrealisedPnl') || 0);
    const market = this.store.marketsBySymbol[p.symbol];
    let notional = parseFloat(v(p, 'positionValue') || 0);
    let unrealizedPnlNative = unrealizedPnl; // PnL in native currency (BTC for inverse)

    // For BybitInverse, all markets should be inverse
    if (market?.isInverse) {
      // For inverse contracts:
      // - API returns size in USD contracts (e.g., 1000 = $1000 worth)
      // - contracts field should show BTC amount (base currency)
      // - notional should show USD value
      const ticker = this.store.tickersBySymbol[p.symbol];
      const referencePrice = ticker?.mark || ticker?.last || entryPrice;
      if (referencePrice && referencePrice > 0) {
        // Convert USD contracts to BTC amount for the contracts field
        // Use direct division with proper precision for BTC (8 decimals)
        const btcAmount = contractsNative / referencePrice;
        // Round to 8 decimal places for BTC precision
        contracts = Math.round(btcAmount * 1e8) / 1e8;

        // Debug logging
        if (contractsNative > 0) {
          this.log(
            `BybitInverse position: ${p.symbol} - USD contracts: ${contractsNative}, BTC price: ${referencePrice}, BTC amount: ${contracts}`,
          );
        }
      }
      // Notional is the USD value
      notional = contractsNative;
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
        contractsNative, // Native contracts (USD for inverse)
        unrealizedPnlNative, // PnL in native currency (BTC for inverse)
      },
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    return position;
  }

  mapOrder(o: Record<string, any>) {
    const stopOrderType = v(o, 'stopOrderType');
    const reduceOnly = v(o, 'reduceOnly') === true || v(o, 'reduceOnly') === 'true';
    const closeOnTrigger = v(o, 'closeOnTrigger') === true || v(o, 'closeOnTrigger') === 'true';
    const tpslMode = v(o, 'tpslMode');
    const isPositionTpslMode = isBybitPositionTpslMode(tpslMode);
    const createTypeOrderType = isPositionTpslMode
      ? POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE[v(o, 'createType')]
      : undefined;
    const isStop = Boolean((stopOrderType && stopOrderType !== 'UNKNOWN') || createTypeOrderType);

    const triggerPrice = v(o, 'triggerPrice');
    const oPrice = isStop ? triggerPrice || o.price : o.price;
    const oType = createTypeOrderType || (isStop ? stopOrderType : v(o, 'orderType'));
    const isOpeningTriggerOrder =
      isStop &&
      !reduceOnly &&
      !closeOnTrigger &&
      !isPositionTpslMode &&
      (parseFloat(triggerPrice ?? '0') > 0 || v(o, 'createType') === 'CreateByStopOrder');
    const type = isOpeningTriggerOrder ? OrderType.Conditional : ORDER_TYPE[oType];
    const timeInForce = ORDER_TIME_IN_FORCE[o.timeInForce];

    const remainingNative = o.leavesQty ? parseFloat(o.leavesQty) : subtract(o.qty ?? 0, v(o, 'cumExecQty') ?? 0);
    const price = parseFloat(oPrice);

    const orders: Order[] = [
      {
        id: o.orderId,
        status: ORDER_STATUS[v(o, 'orderStatus')],
        symbol: o.symbol,
        type,
        side: ORDER_SIDE[o.side],
        ...this.remainingInStoreUnits(remainingNative, price),
        price,
        amount: (oType === 'StopLoss' || oType === 'TakeProfit') && tpslMode === 'Full' ? 0 : parseFloat(o.qty ?? 0),
        filled: parseFloat(v(o, 'cumExecQty') ?? 0),
        reduceOnly,
        close: closeOnTrigger,
        info: o,
        timeInForce,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        ...(type === OrderType.Conditional && { triggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerBy')] }),
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
        ...this.remainingInStoreUnits(remainingNative, sl),
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
        ...this.remainingInStoreUnits(remainingNative, tp),
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

    if (v(data, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data, 'retMsg'));
      return PositionMode.OneWay;
    }

    const formattedData = data?.result?.list?.map(this.mapPosition.bind(this));
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
