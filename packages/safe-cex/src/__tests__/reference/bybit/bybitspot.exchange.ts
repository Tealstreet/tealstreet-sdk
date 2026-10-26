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
  CoinBalance,
  ExchangeOptions,
  LogSeverity,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
} from '../../../types';
import { v } from '../../../utils/get-key';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, subtract } from '../../../utils/safe-math';
import { isUnknownOutcome } from '../../../utils/unknownSendOutcome';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './bybit.api';
import { getBybitOrderExecutionParams } from '../../../exchanges/bybit/bybit.order-params';
import { BYBIT_BRAZIL_SITE_ID } from '../../../exchanges/bybit/bybit.site';
import {
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
  POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE,
} from '../../../exchanges/bybit/bybit.types';
import { BybitPrivateWebsocket } from './bybitspot.ws-private';
import { BybitPublicWebsocket } from './bybitspot.ws-public';

const REINIT_RETRY_MS = 15_000;
const REINIT_RETRY_MAX_MS = 5 * 60_000;

const RECENTLY_PLACED_ORDER_GRACE_MS = 8_000;

const isBybitOrderAlreadyGoneCancelMessage = (message: unknown) => {
  const normalized = `${message || ''}`.toLowerCase();
  return normalized.includes('order not exists') || normalized.includes('too late to cancel');
};

export class BybitSpotExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Bybit spot', /^[A-Za-z0-9_-]{1,36}$/);

  xhr: Axios;
  unlimitedXHR: Axios;

  isSpot = true;

  publicWebsocket: BybitPublicWebsocket;
  privateWebsocket: BybitPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // Bybit no longer offers Classic accounts. Start in UTA 2.0 Pro mode so
  // account validation never probes the retired CONTRACT wallet API first.
  private unifiedMarginStatus: number = 6;

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

  get accountType() {
    return this.unifiedMarginStatus === 1 ? 'CONTRACT' : 'UNIFIED';
  }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BybitPublicWebsocket(this);
    this.privateWebsocket = new BybitPrivateWebsocket(this);
  }

  getTickerSymbolsForPublicStream() {
    return this.getTickerSymbolsForSubscription(this.store.markets.map((market) => market.symbol));
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
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
      remaining: qty,
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

    const nextOrder: Order = {
      ...order,
      ...update,
      ...(typeof update.amount === 'number'
        ? { remaining: Math.max(0, update.amount - (order.filled || 0)) }
        : {}),
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

    const publicDataOnly = this.isPublicDataOnly;

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

    if (!useExternalPublicData) {
      const [, rawTickers] = await Promise.all([
        await this.fetchMarkets().then((markets) => {
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
        `Bybit market data is ready, but the private API was rejected (${privateProbeFailure}) — trading is unavailable`,
        LogSeverity.Warning,
      );
    } else {
      this.log(`Ready to trade on Bybit`);
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

      // Always `[]` — bybit spot reports no positions. Kept because it is what
      // marks `loaded.positions`, and `addOrUpdatePositionsFull` is the documented
      // account-wide shape even when the list is empty.
      const positions = await this.fetchPositions();
      if (this.isDisposed) return;

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

    if (v(data, 'retMsg') !== 'OK') {
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
      return this.store.balance;
    }
    const [firstAccount] = rawBalance;
    // Spot holds no positions, so there is no unrealized PnL to carry and nothing
    // to subtract out of equity. `Balance.upnl` is required, hence the literal.
    const upnl = 0;
    const total = parseFloat(firstAccount.totalEquity);

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

  fetchOrders = async () => {
    const recursiveFetch = async (
      cursor: string = '',
      orders: Array<Record<string, any>> = [],
    ): Promise<Array<Record<string, any>>> => {
      const { data } = await this.reinitOnThrow(
        this.xhr.get(ENDPOINTS.UNFILLED_ORDERS, {
          params: {
            category: 'spot',
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

    return this.preserveRecentlyPlacedOrders(orders);
  };

  fetchPositions = async (_symbols?: string[]) => {
    return [];
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
    const { data } = await this.xhr.get(ENDPOINTS.TICKERS, {
      params: { category: 'spot' },
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
        fundingRate: 0,
        fundingTime: 0,
        volume: volume24h,
        quoteVolume: volume24h * lastPrice,
      });
    }
    return tickers;
  };

  fetchMarkets = async () => {
    const markets = await this.fetchMarketsHelper('spot');
    const newMarkets = markets.flat();
    if (newMarkets.some((m) => m === null)) {
      this.emitAndLogError('Failed to fetch markets');
      return this.store.markets;
    }
    return newMarkets as Market[];
  };

  fetchMarketsHelper = async (category: string) => {
    const { data } = await this.xhr.get(ENDPOINTS.MARKETS, {
      params: { category, limit: 1000 },
    });

    if (v(data, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data, 'retMsg'));
      return [null];
    }

    const markets: Market[] = data.result.list
      // .filter((market: Record<string, any>) => market.quoteCoin === 'USDT')
      .map((market: Record<string, any>) => {
        return {
          // old id isn't unique with expiring usdc futs
          // id: `${market.baseCoin}/${market.quoteCoin}:${market.settleCoin}`,
          id: market.symbol,
          symbol: market.symbol,
          base: market.baseCoin,
          quote: market.quoteCoin,
          active: market.status === 'Trading',
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
              min: 1,
              max: 1,
            },
          },
          isInverse: category === 'inverse',
          // Stamp `isSpot` on the Market, not just the class field. The
          // class-level `BaseExchange.isSpot` has no consumers — every
          // spot check in web and safe-cex reads `market.isSpot` — so
          // without this, spot guards (leverage/margin rows, size
          // rounding, TP/SL preview) silently fail open on this venue.
          isSpot: true,
          // isReadOnly: category === 'inverse',
          isExpiring: false,
          info: market,
        };
      });

    return markets;
  };

  // No `fetchClosedPnl` on spot. Bybit's closed-PnL endpoint is position-based and does
  // not serve `category: 'spot'`, so there is nothing to call. The dead Tealstreet-cache
  // version that used to live here always returned []. `BybitSpotMetadata` therefore sets
  // `supportsClosedPnLEndpoint: false` — it inherits `true` from BybitMetadata via the
  // spread, and leaving it would send the UI down the endpoint branch and render the
  // Closed Positions tab permanently empty.

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 500, 1000);
    const interval = INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const params = {
      category: 'spot',
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

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
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

  private getOrderExecutionParams = (opts: PlaceOrderOpts) => {
    try {
      return getBybitOrderExecutionParams(opts, 'spot');
    } catch (error: any) {
      const message = error?.message || `${error}`;
      this.emitAndLogError(message);
      throw error;
    }
  };

  placeOrder = async (opts: BybitPlaceOrderOpts) => {
    const clientOrderId = this.resolveCallerClientOrderId(opts);
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

    const positionIdx = await this.getOrderPositionIdx(opts);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const price = opts.price ? adjust(opts.price, pPrice) : null;
    const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
    const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;
    const timeInForce = ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
    const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;

    let triggerDirection: number | undefined = undefined;
    if (triggerPrice) {
      const ticker = this.store.tickersBySymbol[opts.symbol]!;
      if (triggerPrice > ticker.mark) {
        triggerDirection = 1;
      } else {
        triggerDirection = 2;
      }
    }

    let type = opts.type;
    if (triggerPrice && (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)) {
      type = OrderType.Market;
    }

    const req = omitUndefined({
      category: 'spot',
      symbol: opts.symbol,
      side: inverseObj(ORDER_SIDE)[opts.side],
      orderType: inverseObj(ORDER_TYPE)[type],
      qty: `${amount}`,
      price: opts.type === OrderType.Limit ? `${price}` : undefined,
      stopLoss: opts.stopLoss ? `${stopLoss}` : undefined,
      takeProfit: opts.takeProfit ? `${takeProfit}` : undefined,
      reduceOnly: opts.reduceOnly || false,
      triggerPrice: opts.triggerPrice ? `${triggerPrice}` : undefined,
      triggerDirection: triggerDirection ? `${triggerDirection}` : undefined,
      triggerBy:
        opts.type === OrderType.StopLoss
          ? opts.slTriggerType === OrderStopTriggerType.LastPrice
            ? 'LastPrice'
            : 'MarkPrice'
          : opts.type === OrderType.TakeProfit
            ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
              ? 'MarkPrice'
              : 'LastPrice'
            : undefined,
      slTriggerBy: opts.stopLoss
        ? opts.slTriggerType === OrderStopTriggerType.LastPrice
          ? 'LastPrice'
          : 'MarkPrice'
        : undefined,
      tpTriggerBy: opts.takeProfit
        ? opts.slTriggerType === OrderStopTriggerType.MarkPrice
          ? 'MarkPrice'
          : 'LastPrice'
        : undefined,
      timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
      closeOnTrigger: false,
      positionIdx,
      ...orderExecutionParams,
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);

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

  placeOrders = async (orders: BybitPlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    for (const order of orders) {
      this.getOrderExecutionParams(order);
    }

    const orderIds = await mapSeries(orders, (order) => this.placeOrder(order));
    return orderIds.flat();
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
      category: 'spot',
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
    }

    return [data.result.orderId];
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
      category: 'spot',
      symbol: opts.symbol,
      positionIdx: await this.getStopOrderPositionIdx(opts),
      trailingStop: `${distance}`,
    };

    const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

    if (data.retMsg !== 'OK') {
      this.emitAndLogError(data.retMsg);
    }

    return [data.result.orderId];
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
      'price' in update
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
          category: 'spot',
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
      (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit) &&
      Boolean(order.info?.triggerPrice);

    // If we want to update the price or amount of a limit order
    // we can do it directly on the order
    if (order.type === OrderType.Limit || isConditionalOrder) {
      payload = {
        category: 'spot',
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
        category: 'spot',
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
        const category = 'spot';
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
      category: 'spot',
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

  mapOrder(o: Record<string, any>) {
    const stopOrderType = v(o, 'stopOrderType');
    const tpslMode = v(o, 'tpslMode');
    const isPositionTpslMode = tpslMode === 'Full' || tpslMode === 'Partial';
    const createTypeOrderType = isPositionTpslMode
      ? POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE[v(o, 'createType')]
      : undefined;
    const isStop = Boolean((stopOrderType && stopOrderType !== 'UNKNOWN') || createTypeOrderType);

    const triggerPrice = v(o, 'triggerPrice');
    const oPrice = isStop ? triggerPrice || o.price : o.price;
    const oType = createTypeOrderType || (isStop ? stopOrderType : v(o, 'orderType'));

    const type = ORDER_TYPE[oType];
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
        reduceOnly: v(o, 'reduceOnly') || false,
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

  private getOrderPositionIdx = async (_opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'symbol'>) => {
    return 0;
  };

  private getStopOrderPositionIdx = async (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'symbol'>) => {
    const positionIdx = await this.getOrderPositionIdx(omit(opts, ['reduceOnly']));
    return { 0: 0, 1: 2, 2: 1 }[positionIdx];
  };
}
