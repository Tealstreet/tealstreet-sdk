import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';

import createHash from 'create-hash';
import chunk from 'lodash/chunk';
import uniqBy from 'lodash/uniqBy';

import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Balance,
  Candle,
  ExchangeAccount,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  FetchedLeverage,
  LogSeverity,
  MarginType,
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
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { createAPI } from './mexc.api';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  REVERSE_MARGIN_TYPE,
  REVERSE_ORDER_SIDE,
  REVERSE_ORDER_TYPE,
  REVERSE_POSITION_MODE,
  REVERSE_TIME_IN_FORCE,
} from './mexc.types';
import {
  fakePositionsForMarket,
  fromMexcSymbol,
  isTradableMexcMarket,
  normalizeBalance,
  normalizeExecution,
  normalizeKlineResponse,
  normalizeMarginType,
  normalizeMarket,
  nativeVolumeToAmount,
  normalizeOrder,
  normalizeOrderBook,
  normalizePlanOrder,
  normalizePosition,
  normalizePositionMode,
  normalizeStopOrder,
  normalizeTimestampToSeconds,
  normalizeTicker,
  toIntegerString,
  toMexcSymbol,
  toNumber,
} from './mexc.utils';
import { MexcPrivateWebsocket } from './mexc.ws-private';
import { MexcPublicWebsocket } from './mexc.ws-public';

const extractData = <T = any>(payload: any): T => (payload?.data ?? payload) as T;

const extractRows = <T = Record<string, any>>(payload: any, keys: string[] = []): T[] => {
  const data = extractData(payload);
  if (Array.isArray(data)) return data;
  for (const key of keys) {
    if (Array.isArray(data?.[key])) return data[key];
    if (Array.isArray(data?.data?.[key])) return data.data[key];
  }
  return [];
};

type MexcSymbolConfig = {
  marginType: MarginType;
  buyLeverage: number;
  sellLeverage: number;
  positionMode: PositionMode;
};

const MEXC_TRIGGER_GTE = 1;
const MEXC_TRIGGER_LTE = 2;
const MEXC_EXECUTE_CYCLE_SEVEN_DAYS = 2;
const MEXC_TRIGGER_TREND_LATEST_PRICE = 1;
const MEXC_PLAN_ORDER_LOOKBACK_MS = 89 * 24 * 60 * 60 * 1000;
const MEXC_REGULAR_ORDER_SUBMIT_GAP_MS = 450;
const MEXC_SCALED_ORDER_SUBMIT_GAP_MS = 1_500;
const MEXC_REGULAR_ORDER_THROTTLE_COOLDOWN_MS = 1_500;
const MEXC_RECENT_STOP_CANCEL_SHIELD_MS = 15_000;
// Preservation window for open regular orders that the WS has already
// written to the store but MEXC's `open_orders` REST endpoint has not yet
// reflected. The 2s periodic `tick` wholesale-replaces `orders` with the
// REST snapshot, so without this window a just-placed / just-moved order
// briefly disappears from the UI until the next tick's REST catches up.
// The bound is generous enough to cover normal REST propagation lag and
// tight enough that a WS-missed external cancellation still clears
// within one window; WS reconnect's `fetchAndSetPrivateSnapshot` remains
// the wholesale reconciliation authority.
const MEXC_RECENT_REGULAR_ORDER_PRESERVE_MS = 15_000;
// Same idea as `MEXC_RECENT_REGULAR_ORDER_PRESERVE_MS`, but for plan and
// stop orders. These have their own REST endpoints (`planorder/list` and
// `stoporder/list`) that are noticeably laggier than `open_orders`, so the
// window is a bit longer to keep the chart line stable during rapid TP/SL
// drag/place bursts. Entries are cleared eagerly on the first WS/REST
// sighting or on explicit cancel to keep the shield small.
const MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS = 20_000;
// Trailing-debounce for `fetchAndSetOrders` after placing / moving TP/SL.
// The window fires once per burst per symbol so rapid drags emit one REST
// reconcile instead of one per drag frame.
const MEXC_STOP_SNAPSHOT_REFRESH_DELAY_MS = 1_500;
// Shield window for orders MEXC just rejected server-side by delivering a
// `state=4` (Canceled) frame with a non-zero `errorCode` and `dealVol=0`
// right after acceptance. Common trigger: a reduce-only order whose total
// open close size exceeds the open position — MEXC accepts each such
// order briefly then cancels it. Because the HTTP `200` typically arrives
// AFTER both WS frames, the placement site's optimistic-add path (and the
// tick's preservation window) would otherwise resurrect the row as Open.
// This shield lets subsequent code recognize the id and skip resurrection.
const MEXC_RECENT_REJECTED_ORDER_SHIELD_MS = 30_000;

// Best-effort translation of MEXC futures `errorCode` values seen on
// `state=4` (Canceled) `push.personal.order` frames. MEXC does not publish
// a canonical enum for this field, so any code not covered here falls
// back to the numeric code in the user-facing message.
const MEXC_ORDER_CANCEL_REASONS: Record<number, string> = {
  6: 'total reduce-only close exceeds open short position',
  7: 'total reduce-only close exceeds open long position',
  8: 'order rejected by venue',
  10: 'venue rejected the order (commonly: reduce-only would over-close, or post-only would cross the book)',
  11: 'IOC / FOK order canceled by matching rule',
};

export class MexcExchange extends BaseExchange {
  xhr: Axios;
  publicWebsocket: MexcPublicWebsocket;
  privateWebsocket: MexcPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private accountPositionMode: PositionMode = PositionMode.Hedged;
  private symbolSettings = new Map<string, MexcSymbolConfig>();
  private regularOrderSubmitQueue: Promise<void> = Promise.resolve();
  private lastRegularOrderSubmitAt = 0;
  private regularOrderSubmitCooldownUntil = 0;
  private nativeBatchOrdersAvailable: boolean | undefined;
  private recentlyCanceledStopLikeOrderIds = new Map<string, number>();
  private recentlyRejectedRegularOrderIds = new Map<string, number>();
  // Shield map for plan and stop orders whose HTTP `POST /placeOrder` just
  // succeeded but whose corresponding WS frame / REST snapshot has not yet
  // caught up. Entries are keyed by order id (with the optional `__side`
  // suffix stripped) and hold the placement timestamp. Reads via
  // `isRecentlyPlacedStopLikeOrder` self-expire past
  // `MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS`. Populated by
  // `addOptimisticStopOrder`, cleared eagerly by `markStopLikeOrderSeen`
  // (WS + REST reconcile paths) and by `noteRecentlyCanceledStopLikeOrders`.
  private recentlyPlacedStopLikeOrderIds = new Map<string, number>();
  // Per-symbol trailing-debounce handles for `scheduleStopOrderSnapshotRefresh`.
  // Keyed by internal (non-venue) symbol; cleared on fire, on `dispose`, and
  // whenever a superseding call for the same symbol arrives during the
  // trailing window.
  private pendingStopSnapshotRefreshTimers = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI);
    this.publicWebsocket = new MexcPublicWebsocket(this);
    this.privateWebsocket = new MexcPrivateWebsocket(this);
  }

  validateAccount = async () => {
    try {
      const balance = await this.fetchBalance();
      return { error: '', balance };
    } catch (err: any) {
      const message = err?.response?.data?.message || err?.response?.data?.msg || err?.message || 'Invalid MEXC key';
      this.emitAndLogError(message);
      return { error: message };
    }
  };

  getAccount = async (): Promise<ExchangeAccount> => {
    const validation = await this.validateAccount();
    if (validation.error) throw new Error(validation.error);

    const userId = createHash('sha256')
      .update(this.options.key || 'mexc')
      .digest('hex')
      .slice(0, 24);
    return { userId: `mexc:${userId}` };
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;
      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;
      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      this.publicWebsocket.connectAndSubscribe();
      this.log(`Loaded ${Math.min(markets.length, tickers.length)} MEXC markets`);
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    try {
      this.accountPositionMode = await this.fetchPositionMode();
      this.store.setSetting('isHedged', this.accountPositionMode === PositionMode.Hedged);
    } catch (err: any) {
      this.log(`Failed to load MEXC position mode: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
    }

    await Promise.all([
      this.fetchBalanceAndPositions().then(({ balance, positions }) => {
        if (this.isDisposed) return;
        this.store.update({
          balance,
          positions,
          loaded: { ...this.store.loaded, balance: true, positions: true },
        });
      }),
      this.fetchOrders().then((orders) => {
        if (this.isDisposed) return;
        this.store.update({
          orders,
          loaded: { ...this.store.loaded, orders: true },
        });
      }),
    ]);

    await this.tick();
    this.log('Ready to trade on MEXC');
  };

  tick = async () => {
    if (this.isDisposed || this.isPublicDataOnly) return;

    try {
      const { balance, positions } = await this.fetchBalanceAndPositions();
      if (this.isDisposed) return;
      this.store.addOrUpdatePositionsFull(positions);
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true, positions: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
    }

    try {
      await this.fetchAndSetRegularOrders();
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
    }

    if (!this.isDisposed) {
      loop(() => this.tick(), this.getTickInterval());
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.CONTRACT_DETAIL);
      return extractRows<Record<string, any>>(data)
        .filter(isTradableMexcMarket)
        .map(normalizeMarket);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.TICKER);
      const tickersById = new Map(extractRows<Record<string, any>>(data).map((row) => [String(row.symbol), row]));
      return this.store.markets.reduce<Ticker[]>((acc, market) => {
        const raw = tickersById.get(market.id);
        if (raw) acc.push(normalizeTicker(raw, market));
        return acc;
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  fetchBalance = async (): Promise<Balance> => {
    const { data } = await this.xhr.get(ENDPOINTS.ASSETS);
    return normalizeBalance(extractRows(data));
  };

  fetchBalanceAndPositions = async () => {
    const [balance, positions] = await Promise.all([this.fetchBalance(), this.fetchPositions()]);
    return { balance, positions };
  };

  fetchPositionMode = async (): Promise<PositionMode> => {
    const { data } = await this.xhr.get(ENDPOINTS.POSITION_MODE);
    return normalizePositionMode(extractData(data));
  };

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    if (this.isPublicDataOnly) return [];

    try {
      const { data } = await this.xhr.get(ENDPOINTS.OPEN_POSITIONS);
      const requestedSymbols = symbols?.length ? symbols : this.store.markets.map((market) => market.symbol);
      const requestedSet = new Set(requestedSymbols);

      const openPositions = extractRows<Record<string, any>>(data)
        .map((row) =>
          normalizePosition(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)], this.accountPositionMode),
        )
        .filter((position): position is Position => !!position && requestedSet.has(position.symbol));

      const openIds = new Set(openPositions.map((position) => position.id));
      const fakePositions = requestedSymbols.flatMap((symbol) => {
        const market = this.store.marketsBySymbol[symbol];
        if (!market) return [];
        const config = this.symbolSettings.get(symbol);
        return fakePositionsForMarket(
          market,
          config?.positionMode || this.accountPositionMode,
          config?.marginType || MarginType.Cross,
          config
            ? {
                [PositionSide.Long]: config.buyLeverage,
                [PositionSide.Short]: config.sellLeverage,
              }
            : 20,
        ).filter((position) => !openIds.has(position.id));
      });

      return uniqBy([...openPositions, ...fakePositions], 'id');
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return symbols ? this.store.positions.filter((position) => symbols.includes(position.symbol)) : this.store.positions;
    }
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    if (this.isPublicDataOnly) return [];

    const orders = await this.fetchRegularOrders(symbols);
    const requestedSymbols = symbols?.length ? symbols : [undefined];

    for (const symbol of requestedSymbols) {
      const venueSymbol = symbol ? toMexcSymbol(symbol) : undefined;

      try {
        const now = virtualClock.getCurrentTimeMs();
        const { data } = await this.xhr.get(ENDPOINTS.PLAN_ORDER_LIST, {
          params: omitUndefined({
            symbol: venueSymbol,
            states: '1',
            start_time: now - MEXC_PLAN_ORDER_LOOKBACK_MS,
            end_time: now,
            page_num: 1,
            page_size: 100,
          }),
        });
        orders.push(
          ...extractRows<Record<string, any>>(data, ['orders', 'items'])
            .map((row) => normalizePlanOrder(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
            .filter((order): order is Order => !!order),
        );
      } catch (err: any) {
        this.log(`Failed to fetch MEXC plan orders: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
      }

      try {
        const now = virtualClock.getCurrentTimeMs();
        const { data } = await this.xhr.get(ENDPOINTS.STOP_ORDER_LIST, {
          params: omitUndefined({
            symbol: venueSymbol,
            is_finished: 0,
            start_time: now - MEXC_PLAN_ORDER_LOOKBACK_MS,
            end_time: now,
            page_num: 1,
            page_size: 100,
          }),
        });
        orders.push(
          ...extractRows<Record<string, any>>(data, ['orders', 'items'])
            .map((row) => normalizeStopOrder(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
            .filter((order): order is Order => !!order),
        );
      } catch (err: any) {
        this.log(`Failed to fetch MEXC position TP/SL orders: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
      }
    }

    return this.filterRecentlyCanceledStopLikeOrders(uniqBy(orders, 'id'));
  };

  fetchAndSetOrders = async (symbols?: string[]): Promise<Order[]> => {
    const fetched = await this.fetchOrders(symbols);
    if (this.isDisposed) return fetched;
    const orders = fetched.map((order) => this.mergeOrderSnapshot(order));

    // Any id we see in this snapshot is now venue-authoritative — release the
    // placement shield entry so subsequent snapshots aren't second-guessed.
    for (const order of orders) {
      this.markStopLikeOrderSeen(order.id);
    }

    if (symbols?.length) {
      for (const symbol of symbols) {
        const symbolOrders = orders.filter((order) => order.symbol === symbol);
        const fetchedIds = new Set(symbolOrders.map((order) => order.id));
        const existingOpenOrders = this.store.orders.filter(
          (order) =>
            order.symbol === symbol &&
            order.status === OrderStatus.Open &&
            !fetchedIds.has(order.id) &&
            !this.isStopLikeOrder(order) &&
            !this.isRecentlyCanceledStopLikeOrder(order),
        );
        // MEXC's `planorder/list` and `stoporder/list` snapshots are visibly
        // laggier than `open_orders`. During rapid TP/SL bursts, a snapshot
        // may not include a row that HTTP has already returned an id for
        // (and the private WS frame may still be inflight). Preserve any
        // shielded, still-open, non-canceled stop-like row from being
        // dropped by the wholesale replace below. See Bybit's
        // `preserveRecentlyPlacedOrders` for the analogous pattern.
        const shieldedStopLikeOrders = this.store.orders.filter(
          (order) =>
            order.symbol === symbol &&
            order.status === OrderStatus.Open &&
            !fetchedIds.has(order.id) &&
            this.isStopLikeOrder(order) &&
            !this.isRecentlyCanceledStopLikeOrder(order) &&
            this.isRecentlyPlacedStopLikeOrder(order),
        );
        this.store.setOrdersForSymbol(
          symbol,
          uniqBy([...symbolOrders, ...existingOpenOrders, ...shieldedStopLikeOrders], 'id'),
        );
      }
    } else {
      // Full-scope refreshes are authoritative for the whole account, so
      // shielded rows only survive if the venue *and* the shield agree.
      // Merge shielded stop-like rows that the venue did NOT return so the
      // chart stays stable during a full snapshot immediately after a burst.
      const fetchedIds = new Set(orders.map((order) => order.id));
      const shieldedStopLikeOrders = this.store.orders.filter(
        (order) =>
          order.status === OrderStatus.Open &&
          !fetchedIds.has(order.id) &&
          this.isStopLikeOrder(order) &&
          !this.isRecentlyCanceledStopLikeOrder(order) &&
          this.isRecentlyPlacedStopLikeOrder(order),
      );
      this.store.update({
        orders: uniqBy([...orders, ...shieldedStopLikeOrders], 'id'),
        loaded: { ...this.store.loaded, orders: true },
      });
    }

    return orders;
  };

  fetchRegularOrders = async (symbols?: string[]): Promise<Order[]> => {
    if (this.isPublicDataOnly) return [];

    const requestedSymbols = symbols?.length ? symbols : [undefined];
    const orders: Order[] = [];

    for (const symbol of requestedSymbols) {
      const venueSymbol = symbol ? toMexcSymbol(symbol) : undefined;

      try {
        const { data } = await this.xhr.get(this.orderListEndpoint(venueSymbol), {
          params: {
            page_num: 1,
            page_size: 100,
          },
        });
        this.assertCommonSuccess(data, 'MEXC open orders read');
        const payload = extractData(data);
        if (!Array.isArray(payload) && !['orders', 'items'].some((key) => Array.isArray(payload?.[key]) || Array.isArray(payload?.data?.[key]))) {
          throw new Error('Invalid MEXC open orders read');
        }
        for (const row of extractRows<Record<string, any>>(data, ['orders', 'items'])) {
          const order = normalizeOrder(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]);
          if (!order) throw new Error('Invalid MEXC open order row');
          orders.push(order);
        }
      } catch (err: any) {
        this.log(`Failed to fetch MEXC open orders: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
        throw err;
      }
    }

    return uniqBy(orders, 'id');
  };

  fetchAndSetRegularOrders = async (symbols?: string[]): Promise<Order[]> => {
    const fetched = await this.fetchRegularOrders(symbols);
    if (this.isDisposed) return fetched;
    const regularOrders = fetched.map((order) => this.mergeOrderSnapshot(order));

    if (symbols?.length) {
      for (const symbol of symbols) {
        const symbolRegularOrders = regularOrders.filter((order) => order.symbol === symbol);
        const fetchedIds = new Set(symbolRegularOrders.map((order) => order.id));
        const symbolStopLikeOrders = this.store.orders.filter((order) => this.shouldPreserveStopLikeOrder(order, symbol));
        const preservedRegularOrders = this.store.orders.filter((order) =>
          this.shouldPreserveRecentRegularOrder(order, fetchedIds, symbol),
        );
        this.store.setOrdersForSymbol(
          symbol,
          uniqBy([...symbolStopLikeOrders, ...preservedRegularOrders, ...symbolRegularOrders], 'id'),
        );
      }
    } else {
      const fetchedIds = new Set(regularOrders.map((order) => order.id));
      const stopLikeOrders = this.store.orders.filter((order) => this.shouldPreserveStopLikeOrder(order));
      const preservedRegularOrders = this.store.orders.filter((order) =>
        this.shouldPreserveRecentRegularOrder(order, fetchedIds),
      );
      this.store.update({
        orders: uniqBy([...stopLikeOrders, ...preservedRegularOrders, ...regularOrders], 'id'),
        loaded: { ...this.store.loaded, orders: true },
      });
    }

    return regularOrders;
  };

  mergeOrderSnapshot = (order: Order, previous?: Order): Order => {
    const current = previous ?? this.store.orders.find((row) => row.id === order.id);
    if (!current) return order;
    let next = order;
    if (!this.isStopLikeOrder(order) && order.filled < current.filled) {
      next = { ...order, filled: current.filled, remaining: current.remaining, extra: { ...order.extra, ...current.extra } };
    }
    if (current.status === OrderStatus.Closed || (current.status === OrderStatus.Canceled && order.status === OrderStatus.Open)) {
      next = { ...next, status: current.status };
    }
    // Retain presence as well as values: a sparse frame after an explicit
    // clear must not let a later placement acknowledgement restore that leg.
    const info = { ...next.info };
    for (const field of ['stopLossPrice', 'takeProfitPrice', 'lossTrend', 'profitTrend', 'trend']) {
      if (!Object.hasOwn(order.info ?? {}, field) && Object.hasOwn(current.info ?? {}, field)) {
        info[field] = current.info[field];
      }
    }
    // Omitted native fields are unknown; an explicit zero price removes a bracket.
    const slTriggerType = order.slTriggerType ?? current.slTriggerType;
    const tpTriggerType = order.tpTriggerType ?? current.tpTriggerType;
    return {
      ...next,
      info,
      stopLoss: Object.hasOwn(order.info ?? {}, 'stopLossPrice') ? order.stopLoss : order.stopLoss ?? current.stopLoss,
      takeProfit: Object.hasOwn(order.info ?? {}, 'takeProfitPrice') ? order.takeProfit : order.takeProfit ?? current.takeProfit,
      triggerType: order.triggerType ?? (order.type === OrderType.StopLoss ? slTriggerType :
        order.type === OrderType.TakeProfit ? tpTriggerType : undefined) ?? current.triggerType,
      slTriggerType,
      tpTriggerType,
    };
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    const { symbol } = opts;
    if (!symbol) return [];
    if (this.isPublicDataOnly) return [];

    const { data } = await this.xhr.get(ENDPOINTS.ORDER_DEALS, {
      params: omitUndefined({
        symbol: toMexcSymbol(symbol),
        order_id: opts.orderId,
        start_time: opts.from,
        end_time: opts.to,
        page_num: 1,
        page_size: Math.min(Math.max(opts.limit || 100, 1), 100),
      }),
    });

    return extractRows<Record<string, any>>(data, ['orders', 'items'])
      .map((row) => normalizeExecution(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
      .filter((fill): fill is Execution => !!fill)
      .sort((a, b) => b.timestamp - a.timestamp);
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    const interval = INTERVAL[opts.interval];
    if (!interval) throw new Error(`MEXC does not support ${opts.interval} candles`);

    const { data } = await this.xhr.get(`${ENDPOINTS.KLINE}/${toMexcSymbol(opts.symbol)}`, {
      params: omitUndefined({
        interval,
        start: opts.from === undefined ? undefined : normalizeTimestampToSeconds(opts.from),
        end: opts.to === undefined ? undefined : normalizeTimestampToSeconds(opts.to),
      }),
      timeout: 5 * 60 * 1000,
    });

    return normalizeKlineResponse(data).sort((a, b) => a.timestamp - b.timestamp);
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeCandle(opts, callback);
    }
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeOrderbook(symbol, callback);
    }
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchOrderBook = async (symbol: string, limit = 100): Promise<OrderBook & { lastUpdateId?: string }> => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    const { data } = await this.xhr.get(`${ENDPOINTS.DEPTH}/${market.id}`, {
      params: { limit },
    });
    this.assertCommonSuccess(data, 'MEXC depth read');
    const payload = extractData<Record<string, any>>(data);
    const version = toIntegerString(payload?.version);
    const number = (value: unknown): number =>
      typeof value === 'number' || (typeof value === 'string' && /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(value.trim())) ? Number(value) : NaN;
    const validSide = (rows: unknown): boolean => Array.isArray(rows) && rows.every((row) => {
      if (!Array.isArray(row) || row.length < 2) return false;
      const price = number(row[0]);
      const volume = number(row[1]);
      return Number.isFinite(price) && price > 0 && Number.isFinite(volume) && volume >= 0;
    });
    if (!/^\d+$/.test(version) || !validSide(payload?.bids) || !validSide(payload?.asks)) {
      throw new Error('Invalid MEXC depth snapshot');
    }
    return { ...normalizeOrderBook(data, market), lastUpdateId: version };
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    this.resolveCallerClientOrderId(opts);
    if (opts.type === OrderType.TrailingStopLoss || opts.type === OrderType.Conditional) {
      const message = 'MEXC opening conditional and trailing-stop placement is not implemented';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) {
      return [await this.placePlanOrder(opts)];
    }

    await this.ensureSymbolConfig(opts.symbol);
    const payload = this.formatRegularOrderPayload(opts);

    try {
      const { data } = await this.enqueueRegularOrderSubmit(() => this.xhr.post(ENDPOINTS.ORDER_SUBMIT, payload));
      this.assertRegularOrderSubmitSucceeded(data);
      const orderId = toIntegerString(extractData(data)?.orderId ?? extractData(data));
      if (!orderId) throw new Error('MEXC order placement returned no order id');
      this.addOptimisticRegularOrder(opts, orderId);
      if (opts.timeInForce === OrderTimeInForce.PostOnly) {
        this.store.updateOrder({ id: orderId }, { timeInForce: OrderTimeInForce.PostOnly });
      }
      return [orderId];
    } catch (err: any) {
      const message = err?.response?.data?.message || err?.response?.data?.msg || err?.message || 'MEXC order placement failed';
      this.emitAndLogError(message);
      throw err;
    }
  };

  private formatRegularOrderPayload = (opts: PlaceOrderOpts): Record<string, any> => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      const message = `Market ${opts.symbol} not found`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const type =
      opts.type === OrderType.Limit && opts.timeInForce
        ? REVERSE_TIME_IN_FORCE[opts.timeInForce] || REVERSE_ORDER_TYPE[opts.type]
        : REVERSE_ORDER_TYPE[opts.type];
    const nativeVolumeStep = toNumber(market.info?.volUnit, 1);
    const vol = Math.max(toNumber(adjust(opts.amount, nativeVolumeStep), nativeVolumeStep), nativeVolumeStep);
    const side = opts.reduceOnly ? REVERSE_ORDER_SIDE[opts.side].close : REVERSE_ORDER_SIDE[opts.side].open;
    const leverage = this.getLeverageForOrder(opts);
    return omitUndefined({
      symbol: market.id,
      price: opts.type === OrderType.Market ? undefined : `${adjust(opts.price || 0, market.precision.price)}`,
      vol,
      side,
      type,
      openType: REVERSE_MARGIN_TYPE[this.symbolSettings.get(opts.symbol)?.marginType || MarginType.Cross],
      leverage: Math.min(Math.max(leverage, market.limits.leverage.min), market.limits.leverage.max),
      externalOid: `ts_${uuid()}`,
      reduceOnly: opts.reduceOnly || undefined,
      stopLossPrice: opts.stopLoss === undefined ? undefined : `${adjust(opts.stopLoss, market.precision.price)}`,
      takeProfitPrice: opts.takeProfit === undefined ? undefined : `${adjust(opts.takeProfit, market.precision.price)}`,
      lossTrend: opts.stopLoss === undefined ? undefined : this.stopOrderTrend({ ...opts, type: OrderType.StopLoss }),
      profitTrend: opts.takeProfit === undefined ? undefined : this.stopOrderTrend({ ...opts, type: OrderType.TakeProfit }),
    });
  };

  private assertRegularOrderSubmitSucceeded = (payload: any): void => {
    if (payload?.success !== false) return;

    const error = new Error(payload.message || payload.msg || `MEXC order placement failed with code ${payload.code}`);
    (error as any).response = { data: payload };
    this.noteRegularOrderSubmitError(error);
    throw error;
  };

  private enqueueRegularOrderSubmit = async <T>(task: () => Promise<T>): Promise<T> => {
    const run = this.regularOrderSubmitQueue.catch(() => undefined).then(async () => {
      await this.waitForRegularOrderSubmitSlot();
      try {
        return await task();
      } catch (err: any) {
        this.noteRegularOrderSubmitError(err);
        throw err;
      }
    });

    this.regularOrderSubmitQueue = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  };

  private waitForRegularOrderSubmitSlot = async (): Promise<void> => {
    const now = virtualClock.getCurrentTimeMs();
    const earliestSubmitAt = Math.max(
      this.lastRegularOrderSubmitAt + MEXC_REGULAR_ORDER_SUBMIT_GAP_MS,
      this.regularOrderSubmitCooldownUntil,
    );
    const delay = earliestSubmitAt - now;
    if (delay > 0) {
      await sleep(delay);
    }
    this.lastRegularOrderSubmitAt = virtualClock.getCurrentTimeMs();
  };

  private noteRegularOrderSubmitError = (err: any): void => {
    const code = toNumber(err?.response?.data?.code ?? err?.code, Number.NaN);
    const message = String(err?.response?.data?.message ?? err?.response?.data?.msg ?? err?.message ?? '');
    if (code === 510 || message.toLowerCase().includes('requests are too frequent')) {
      this.regularOrderSubmitCooldownUntil = Math.max(
        this.regularOrderSubmitCooldownUntil,
        virtualClock.getCurrentTimeMs() + MEXC_REGULAR_ORDER_THROTTLE_COOLDOWN_MS,
      );
    }
  };

  private placePlanOrder = async (opts: PlaceOrderOpts): Promise<string> => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      const message = `Market ${opts.symbol} not found`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const triggerPrice = opts.triggerPrice ?? opts.price;
    if (!Number.isFinite(triggerPrice) || !triggerPrice || triggerPrice <= 0) {
      const message = `MEXC ${opts.type} requires a positive trigger price`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (opts.amount === 0) {
      return await this.placePositionStopOrder(opts, market, triggerPrice);
    }

    const nativeVolumeStep = toNumber(market.info?.volUnit, 1);
    const vol = toNumber(adjust(opts.amount, nativeVolumeStep), 0);
    if (!Number.isFinite(vol) || vol <= 0) {
      const message = `MEXC ${opts.type} requires a positive explicit amount`;
      this.emitAndLogError(message);
      throw new Error(message);
    }
    const maxNativeVolume = toNumber(market.info?.maxVol ?? market.info?.limitMaxVol, Number.POSITIVE_INFINITY);
    if (Number.isFinite(maxNativeVolume) && vol > maxNativeVolume) {
      const message = `MEXC ${opts.type} amount exceeds maximum native volume ${maxNativeVolume}`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const config = await this.ensureSymbolConfig(opts.symbol);
    const leverage = Math.min(Math.max(this.getLeverageForOrder({ ...opts, reduceOnly: true }), market.limits.leverage.min), market.limits.leverage.max);
    const payload = omitUndefined({
      symbol: market.id,
      vol,
      leverage,
      side: REVERSE_ORDER_SIDE[opts.side].close,
      openType: REVERSE_MARGIN_TYPE[config.marginType],
      triggerPrice: `${adjust(triggerPrice, market.precision.price)}`,
      triggerType: this.planOrderTriggerType(opts),
      executeCycle: MEXC_EXECUTE_CYCLE_SEVEN_DAYS,
      orderType: REVERSE_ORDER_TYPE[OrderType.Market],
      trend: this.stopOrderTrend(opts),
      positionMode: REVERSE_POSITION_MODE[config.positionMode],
      reduceOnly: true,
    });

    try {
      const { data } = await this.xhr.post(ENDPOINTS.PLAN_ORDER_PLACE, payload);
      const orderId = toIntegerString(extractData(data)?.orderId ?? extractData(data));
      if (!orderId) throw new Error('MEXC plan order placement returned no order id');
      this.addOptimisticStopOrder(opts, market, orderId, triggerPrice, '__mexcPlanOrder');
      this.scheduleStopOrderSnapshotRefresh(opts.symbol);
      return orderId;
    } catch (err: any) {
      const message = err?.response?.data?.message || err?.response?.data?.msg || err?.message || 'MEXC plan order placement failed';
      this.emitAndLogError(message);
      throw err;
    }
  };

  private placePositionStopOrder = async (opts: PlaceOrderOpts, market: Market, triggerPrice: number): Promise<string> => {
    const targetSide = opts.side === OrderSide.Sell ? PositionSide.Long : PositionSide.Short;
    let position = this.findOpenPosition(opts.symbol, targetSide, this.store.positions);
    if (!position || !this.nativePositionId(position)) {
      position = this.findOpenPosition(opts.symbol, targetSide, await this.fetchPositions([opts.symbol]));
    }

    if (!position || position.contracts <= 0) {
      const message = `MEXC full-position ${opts.type} requires an open ${targetSide} position on ${opts.symbol}`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const positionId = this.nativePositionId(position);
    if (!positionId) {
      const message = `MEXC full-position ${opts.type} requires a native position id for ${opts.symbol}`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const nativeVolumeStep = toNumber(market.info?.volUnit, 1);
    const nativeAmount = toNumber(
      position.extra?.contractsNative ??
        position.info?.holdVol ??
        position.info?.positionVol ??
        position.info?.vol ??
        position.info?.size,
      0,
    );
    const vol = toNumber(adjust(nativeAmount, nativeVolumeStep), 0);
    if (!Number.isFinite(vol) || vol <= 0) {
      const message = `MEXC full-position ${opts.type} requires a positive native position volume`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const trend = this.stopOrderTrend(opts);
    const adjustedTriggerPrice = `${adjust(triggerPrice, market.precision.price)}`;
    const payload = omitUndefined({
      positionId,
      vol,
      lossTrend: opts.type === OrderType.StopLoss ? trend : MEXC_TRIGGER_TREND_LATEST_PRICE,
      profitTrend: opts.type === OrderType.TakeProfit ? trend : MEXC_TRIGGER_TREND_LATEST_PRICE,
      stopLossPrice: opts.type === OrderType.StopLoss ? adjustedTriggerPrice : undefined,
      takeProfitPrice: opts.type === OrderType.TakeProfit ? adjustedTriggerPrice : undefined,
      profitLossVolType: 'SAME',
      volType: 2,
      takeProfitType: opts.type === OrderType.TakeProfit ? 0 : undefined,
      stopLossType: opts.type === OrderType.StopLoss ? 0 : undefined,
    });

    try {
      const { data } = await this.xhr.post(ENDPOINTS.STOP_ORDER_PLACE, payload);
      const responseData = extractData(data);
      const responseRow = Array.isArray(responseData) ? responseData[0] : responseData;
      const scalarId =
        typeof responseData === 'string' || typeof responseData === 'number' ? responseData : undefined;
      const orderId = toIntegerString(responseRow?.id ?? responseRow?.orderId ?? scalarId);
      if (!orderId) throw new Error('MEXC position TP/SL placement returned no order id');
      this.addOptimisticStopOrder(opts, market, orderId, triggerPrice, '__mexcStopOrder');
      this.scheduleStopOrderSnapshotRefresh(opts.symbol);
      return orderId;
    } catch (err: any) {
      const message =
        err?.response?.data?.message || err?.response?.data?.msg || err?.message || 'MEXC position TP/SL placement failed';
      this.emitAndLogError(message);
      throw err;
    }
  };

  private findOpenPosition = (symbol: string, side: PositionSide, positions: Position[]): Position | undefined =>
    positions.find((position) => position.symbol === symbol && position.side === side && position.contracts > 0);

  // Seed a local Open row for a just-placed regular order so it survives
  // a periodic-tick REST refresh that races the WS `push.personal.order`
  // event. `handleOrder` will overwrite this row (same id) with the real
  // WS payload as soon as it arrives. Market orders are skipped because
  // they typically transition through Open -> Filled fast enough that an
  // optimistic Open row would only flash. `fetchAndSetRegularOrders`
  // preservation is bounded so a stale optimistic row still clears if the
  // venue never confirms the order.
  private addOptimisticRegularOrder = (opts: PlaceOrderOpts, orderId: string): void => {
    if (opts.type === OrderType.Market) return;

    // MEXC can deliver `state=2` (Open) then `state=4` (Canceled) WS frames
    // BEFORE the placement HTTP 200 returns — see `handleOrder`'s rejection
    // path. If the venue has already rejected the order, do not resurrect
    // it as an optimistic Open row; that would flash a phantom line on the
    // chart for the preservation window despite the venue having canceled.
    if (this.isRecentlyRejectedRegularOrder(orderId)) return;
    const existing = this.store.orders.find((order) => order.id === orderId);
    if (existing && (existing.status === OrderStatus.Canceled || existing.status === OrderStatus.Closed)) {
      return;
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) return;

    const nativeVolumeStep = toNumber(market.info?.volUnit, 1);
    const nativeVol = Math.max(toNumber(adjust(opts.amount, nativeVolumeStep), nativeVolumeStep), nativeVolumeStep);
    const amount = nativeVolumeToAmount(nativeVol, market);
    const price = toNumber(adjust(opts.price || 0, market.precision.price));

    const optimistic = {
      id: orderId,
      status: OrderStatus.Open,
      symbol: opts.symbol,
      type: opts.type,
      side: opts.side,
      price,
      amount,
      filled: 0,
      remaining: amount,
      reduceOnly: Boolean(opts.reduceOnly),
      timeInForce: opts.timeInForce || OrderTimeInForce.GoodTillCancel,
      stopLoss: opts.stopLoss === undefined ? undefined : toNumber(adjust(opts.stopLoss, market.precision.price)),
      takeProfit: opts.takeProfit === undefined ? undefined : toNumber(adjust(opts.takeProfit, market.precision.price)),
      slTriggerType: opts.stopLoss === undefined ? opts.slTriggerType :
        opts.slTriggerType ?? opts.triggerType ?? OrderStopTriggerType.LastPrice,
      tpTriggerType: opts.takeProfit === undefined ? opts.tpTriggerType :
        opts.tpTriggerType ?? opts.triggerType ?? OrderStopTriggerType.LastPrice,
      lastUpdate: virtualClock.getCurrentTimeMs(),
      info: {
        __mexcOptimisticRegular: true,
        symbol: market.id,
        vol: nativeVol,
        price: `${price}`,
        ...(opts.stopLoss === undefined ? {} : { stopLossPrice: `${adjust(opts.stopLoss, market.precision.price)}`, lossTrend: this.stopOrderTrend({ ...opts, type: OrderType.StopLoss }) }),
        ...(opts.takeProfit === undefined ? {} : { takeProfitPrice: `${adjust(opts.takeProfit, market.precision.price)}`, profitTrend: this.stopOrderTrend({ ...opts, type: OrderType.TakeProfit }) }),
      },
      extra: {
        remainingNative: nativeVol,
      },
    } as Order;
    // Native WS may precede the HTTP acknowledgement. Its explicit values
    // take precedence; only metadata omitted on the wire falls back to intent.
    this.store.addOrUpdateOrder(existing ? this.mergeOrderSnapshot(existing, optimistic) : optimistic);
  };

  private addOptimisticStopOrder = (
    opts: PlaceOrderOpts,
    market: Market,
    orderId: string,
    triggerPrice: number,
    marker: '__mexcPlanOrder' | '__mexcStopOrder',
  ): void => {
    const nativeAmount = toNumber(opts.amount, 0);
    const amount = nativeAmount === 0 ? 0 : nativeVolumeToAmount(nativeAmount, market);
    const trigger = (opts.type === OrderType.TakeProfit ? opts.tpTriggerType : opts.slTriggerType) ??
      opts.triggerType ?? OrderStopTriggerType.LastPrice;
    this.store.addOrUpdateOrder({
      id: orderId,
      status: OrderStatus.Open,
      symbol: opts.symbol,
      type: opts.type,
      side: opts.side,
      price: toNumber(adjust(triggerPrice, market.precision.price)),
      amount,
      filled: 0,
      remaining: amount,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      triggerType: trigger,
      slTriggerType: opts.type === OrderType.StopLoss ? trigger : opts.slTriggerType,
      tpTriggerType: opts.type === OrderType.TakeProfit ? trigger : opts.tpTriggerType,
      lastUpdate: virtualClock.getCurrentTimeMs(),
      info: {
        [marker]: true,
        symbol: market.id,
        vol: nativeAmount,
      },
      extra: {
        remainingNative: nativeAmount,
      },
    } as Order);
    // Shield the id so a rapid subsequent `fetchAndSetOrders([symbol])` won't
    // wholesale-drop the optimistic row while `planorder/list` or
    // `stoporder/list` still lag the placement HTTP `200`.
    this.notePlacedStopLikeOrder(orderId);
  };

  // Called by the WS `handlePlanOrder` / `handleStopOrder` paths (and by the
  // cancel path) once we see any authoritative signal for the id — venue-owned
  // row now exists (or is gone), so the placement shield is done for that id.
  markStopLikeOrderSeen = (orderId: string): void => {
    if (!orderId) return;
    this.recentlyPlacedStopLikeOrderIds.delete(orderId.split('__')[0]);
  };

  private notePlacedStopLikeOrder = (orderId: string): void => {
    if (!orderId) return;
    this.recentlyPlacedStopLikeOrderIds.set(orderId.split('__')[0], virtualClock.getCurrentTimeMs());
  };

  private isRecentlyPlacedStopLikeOrder = (order: Order): boolean => {
    const id = order.id.split('__')[0];
    const placedAt = this.recentlyPlacedStopLikeOrderIds.get(id);
    if (placedAt === undefined) return false;

    if (virtualClock.getCurrentTimeMs() - placedAt > MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS) {
      this.recentlyPlacedStopLikeOrderIds.delete(id);
      return false;
    }
    return true;
  };

  // Trailing-debounced per-symbol refresh. Rapid TP/SL place/move bursts on
  // MEXC (which has no matched-scope REST endpoint for plan/stop orders)
  // used to fan out one `planorder/list` + one `stoporder/list` per action;
  // this collapses them to one pair of requests per burst, per symbol.
  private scheduleStopOrderSnapshotRefresh = (symbol: string): void => {
    const existing = this.pendingStopSnapshotRefreshTimers.get(symbol);
    if (existing) clearTimeout(existing);

    const timeout = setTimeout(() => {
      this.pendingStopSnapshotRefreshTimers.delete(symbol);
      if (this.isDisposed) return;
      void this.fetchAndSetOrders([symbol]).catch((err: any) => {
        this.log(`Failed to refresh MEXC TP/SL orders: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
      });
    }, MEXC_STOP_SNAPSHOT_REFRESH_DELAY_MS);
    (timeout as ReturnType<typeof setTimeout> & { unref?: () => void }).unref?.();
    this.pendingStopSnapshotRefreshTimers.set(symbol, timeout);
  };

  private nativePositionId = (position: Position): string => {
    const info = position.info as Record<string, any> | undefined;
    return toIntegerString(info?.positionId ?? info?.id);
  };

  private planOrderTriggerType = (opts: PlaceOrderOpts): number => {
    const closesLong = opts.side === OrderSide.Sell;
    if (opts.type === OrderType.TakeProfit) {
      return closesLong ? MEXC_TRIGGER_GTE : MEXC_TRIGGER_LTE;
    }
    return closesLong ? MEXC_TRIGGER_LTE : MEXC_TRIGGER_GTE;
  };

  private stopOrderTrend = (opts: PlaceOrderOpts): number => {
    const triggerType = (opts.type === OrderType.TakeProfit ? opts.tpTriggerType : opts.slTriggerType) ?? opts.triggerType;
    if (triggerType === OrderStopTriggerType.MarkPrice) return 2;
    if (triggerType === OrderStopTriggerType.IndexPrice) return 3;
    return MEXC_TRIGGER_TREND_LATEST_PRICE;
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    if (orders.some((order) => order.type === OrderType.Conditional || order.type === OrderType.TrailingStopLoss)) {
      throw new Error('MEXC opening conditional and trailing-stop placement is not implemented');
    }
    if (!orders.length) return [];
    if (orders.every(this.isRegularPlaceOrder)) {
      if (this.nativeBatchOrdersAvailable !== false) {
        try {
          const orderIds = await this.placeRegularOrdersBatch(orders);
          this.nativeBatchOrdersAvailable = true;
          return orderIds;
        } catch (err: any) {
          if (!this.isNativeBatchAccessDeniedError(err)) {
            throw err;
          }
          this.nativeBatchOrdersAvailable = false;
          this.log('MEXC native batch order placement is unavailable for this account; falling back to paced sequential placement.');
        }
      }
      return await this.placeOrdersSequentially(orders, MEXC_SCALED_ORDER_SUBMIT_GAP_MS);
    }

    return await this.placeOrdersSequentially(orders);
  };

  private placeOrdersSequentially = async (orders: PlaceOrderOpts[], gapMs = 0): Promise<string[]> => {
    const ids: string[] = [];
    for (let index = 0; index < orders.length; index += 1) {
      try {
        ids.push(...(await this.placeOrder(orders[index])));
        if (gapMs > 0 && index < orders.length - 1) {
          await sleep(gapMs);
        }
      } catch (err: any) {
        if (ids.length > 0 && err && typeof err === 'object') {
          err.partialOrderIds = [...ids];
          err.partialOrderCount = ids.length;
          err.totalOrderCount = orders.length;
          err.failedOrderIndex = index;
        }
        throw err;
      }
    }
    return ids;
  };

  private isRegularPlaceOrder = (order: PlaceOrderOpts): boolean =>
    order.type === OrderType.Limit || order.type === OrderType.Market;

  private placeRegularOrdersBatch = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    await Promise.all([...new Set(orders.map((order) => order.symbol))].map((symbol) => this.ensureSymbolConfig(symbol)));

    const payload = orders.map((order) => ({
      ...this.formatRegularOrderPayload(order),
      stpMode: 0,
    }));

    try {
      const { data } = await this.xhr.post(ENDPOINTS.ORDER_SUBMIT_BATCH, payload);
      this.assertRegularOrderBatchSucceeded(data, orders.length);
      const ids = extractRows<Record<string, any>>(data)
        .map((row) => toIntegerString(row.orderId ?? row.id))
        .filter(Boolean);
      if (!ids.length) throw new Error('MEXC batch order placement returned no order ids');

      orders.forEach((order, index) => {
        const orderId = ids[index];
        if (!orderId) return;
        this.addOptimisticRegularOrder(order, orderId);
        if (order.timeInForce === OrderTimeInForce.PostOnly) {
          this.store.updateOrder({ id: orderId }, { timeInForce: OrderTimeInForce.PostOnly });
        }
      });
      return ids;
    } catch (err: any) {
      throw err;
    }
  };

  private isNativeBatchAccessDeniedError = (err: any): boolean => {
    const code = toNumber(err?.response?.data?.code ?? err?.code, Number.NaN);
    const message = String(err?.response?.data?.message ?? err?.response?.data?.msg ?? err?.message ?? '').toLowerCase();
    return (
      code === 511 ||
      message.includes('do not have access') ||
      message.includes('access to the interface') ||
      message.includes('market maker')
    );
  };

  private assertRegularOrderBatchSucceeded = (payload: any, totalOrderCount: number): void => {
    if (payload?.success === false) {
      const error = new Error(payload.message || payload.msg || `MEXC batch order placement failed with code ${payload.code}`);
      (error as any).response = { data: payload };
      throw error;
    }

    const rows = extractRows<Record<string, any>>(payload);
    const failedRows = rows.filter((row) => toNumber(row.errorCode, 0) !== 0);
    if (!failedRows.length) return;

    const partialOrderIds = rows
      .filter((row) => toNumber(row.errorCode, 0) === 0)
      .map((row) => toIntegerString(row.orderId ?? row.id))
      .filter(Boolean);
    const message = failedRows
      .map((row, index) => `${index + 1}: ${row.errorMsg || row.message || row.msg || row.errorCode}`)
      .join(', ');
    const error = new Error(`MEXC batch order placement failed: ${message}`);
    if (partialOrderIds.length) {
      (error as any).partialOrderIds = partialOrderIds;
      (error as any).partialOrderCount = partialOrderIds.length;
      (error as any).totalOrderCount = totalOrderCount;
      (error as any).failedOrderIndex = rows.findIndex((row) => toNumber(row.errorCode, 0) !== 0);
    }
    throw error;
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts): Promise<string[]> => {
    const nextOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      amount: 'amount' in update ? update.amount : this.getReplacementOrderAmount(order),
      price: 'price' in update ? update.price : order.price,
      reduceOnly: order.reduceOnly,
      timeInForce: order.timeInForce,
      triggerType: order.triggerType,
      stopLoss: order.stopLoss,
      takeProfit: order.takeProfit,
      slTriggerType: order.slTriggerType,
      tpTriggerType: order.tpTriggerType,
    };

    await this.cancelOrders([order]);
    if (this.store.orders.some((current) => current.id === order.id && current.status === OrderStatus.Closed)) {
      throw new Error('MEXC original order filled during amend');
    }
    if (!this.isStopLikeOrder(order)) {
      const terminal = await this.readCanceledOrder(order);
      nextOrder.stopLoss = terminal.stopLoss;
      nextOrder.takeProfit = terminal.takeProfit;
      nextOrder.slTriggerType = terminal.slTriggerType;
      nextOrder.tpTriggerType = terminal.tpTriggerType;
      if (!('amount' in update)) nextOrder.amount = toNumber(terminal.extra?.remainingNative, Number.NaN);
      if (!Number.isFinite(nextOrder.amount) || nextOrder.amount <= 0) {
        throw new Error('MEXC replacement requires positive remaining native volume');
      }
    }
    return await this.placeOrder(nextOrder);
  };

  private readCanceledOrder = async (original: Order): Promise<Order> => {
    for (let attempt = 0; attempt <= 5; attempt += 1) {
      const { data } = await this.xhr.get(`/api/v1/private/order/get/${original.id.split('__')[0]}`);
      this.assertCommonSuccess(data, 'MEXC amend reconciliation');
      const raw = extractData<Record<string, any>>(data);
      const order = normalizeOrder(raw, this.store.marketsBySymbol[original.symbol]);
      const volume = toNumber(raw?.vol, Number.NaN);
      const filled = toNumber(raw?.dealVol, Number.NaN);
      if (!order || order.id !== original.id || order.symbol !== original.symbol ||
          !Number.isFinite(volume) || volume <= 0 || !Number.isFinite(filled) || filled < 0 || filled > volume) {
        throw new Error('Invalid MEXC amend order read; replacement withheld');
      }
      const current = this.store.orders.find((row) => row.id === original.id);
      if (order.filled < Math.max(original.filled, current?.filled ?? 0)) {
        throw new Error('MEXC amend read regressed cumulative fills; replacement withheld');
      }
      if (order.status === OrderStatus.Closed) {
        if (current?.status !== OrderStatus.Closed || current.filled < order.filled) this.store.addOrUpdateOrder(order);
        this.store.markOrdersTerminal([{ id: order.id }], OrderStatus.Closed);
        throw new Error('MEXC original order filled during amend');
      }
      if (current?.status === OrderStatus.Closed) throw new Error('MEXC original order filled during amend');
      const state = toNumber(raw.state, Number.NaN);
      if (state === 4) {
        const reconciled = this.mergeOrderSnapshot(order, current ?? original);
        this.store.addOrUpdateOrder(reconciled);
        this.store.markOrdersTerminal([{ id: order.id }], OrderStatus.Canceled);
        return reconciled;
      }
      if ((state !== 1 && state !== 2) || attempt === 5) {
        throw new Error('MEXC cancellation is not authoritative; replacement withheld');
      }
      await sleep(200);
    }
    throw new Error('MEXC cancellation is not authoritative; replacement withheld');
  };

  private getReplacementOrderAmount = (order: Order): number => {
    if (this.isStopOrder(order) && order.amount === 0 && order.remaining === 0) return 0;
    const nativeAmount = toNumber(order.extra?.remainingNative, Number.NaN);
    if (Number.isFinite(nativeAmount) && nativeAmount > 0) return nativeAmount;

    const info = order.info as Record<string, any> | undefined;
    const rawNativeAmount = toNumber(info?.vol ?? info?.quantity ?? info?.origQty, Number.NaN);
    if (Number.isFinite(rawNativeAmount) && rawNativeAmount > 0) return rawNativeAmount;

    return order.amount;
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    if (!orders.length) return;

    const stopOrders = orders.filter((order) => this.isStopOrder(order));
    const planOrders = orders.filter((order) => this.isPlanOrder(order));
    const regularOrders = orders.filter((order) => !this.isStopLikeOrder(order));
    const acceptedStopLikeCancels: Order[] = [];

    for (const lot of chunk(stopOrders, 50)) {
      const { data } = await this.xhr.post(
        ENDPOINTS.STOP_ORDER_CANCEL,
        lot.map((order) => ({ stopPlanOrderId: order.id.split('__')[0] })),
      );
      this.assertCommonSuccess(data, 'MEXC position TP/SL cancellation');
      acceptedStopLikeCancels.push(...lot);
    }

    for (const lot of chunk(planOrders, 50)) {
      const { data } = await this.xhr.post(
        ENDPOINTS.PLAN_ORDER_CANCEL,
        lot.map((order) => ({ symbol: toMexcSymbol(order.symbol), orderId: order.id.split('__')[0] })),
      );
      this.assertCommonSuccess(data, 'MEXC plan order cancellation');
      acceptedStopLikeCancels.push(...lot);
    }

    for (const lot of chunk(regularOrders, 50)) {
      const { data } = await this.xhr.post(
        ENDPOINTS.ORDER_CANCEL,
        lot.map((order) => order.id.split('__')[0]),
      );
      this.assertRegularCancelSucceeded(data);
    }

    this.noteRecentlyCanceledStopLikeOrders(acceptedStopLikeCancels);
    this.store.markOrdersTerminal(
      orders.filter((order) => !this.store.orders.some((current) => current.id === order.id && current.status === OrderStatus.Closed)).map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
  };

  private shouldPreserveStopLikeOrder = (order: Order, symbol?: string): boolean =>
    this.isStopLikeOrder(order) &&
    order.status === OrderStatus.Open &&
    (symbol === undefined || order.symbol === symbol) &&
    !this.isRecentlyCanceledStopLikeOrder(order);

  // Preserve an in-store open regular order across a periodic tick's
  // wholesale REST replace when MEXC's `open_orders` endpoint has not yet
  // reflected a WS-delivered or just-placed order. Bounded by
  // `MEXC_RECENT_REGULAR_ORDER_PRESERVE_MS` so orders whose cancellation
  // we missed still clear within one window.
  private shouldPreserveRecentRegularOrder = (
    order: Order,
    fetchedIds: Set<string>,
    symbol?: string,
  ): boolean => {
    if (fetchedIds.has(order.id)) return false;
    if (order.status !== OrderStatus.Open) return false;
    if (this.isStopLikeOrder(order)) return false;
    if (symbol !== undefined && order.symbol !== symbol) return false;
    // Venue-rejected orders must not be preserved — the row exists locally
    // only because a stale optimistic-add or WS state=2 frame beat the
    // state=4 rejection. See `noteRejectedRegularOrder`.
    if (this.isRecentlyRejectedRegularOrder(order.id)) return false;
    const lastUpdate = toNumber(order.lastUpdate, 0);
    if (!lastUpdate) return false;
    return virtualClock.getCurrentTimeMs() - lastUpdate <= MEXC_RECENT_REGULAR_ORDER_PRESERVE_MS;
  };

  private isStopLikeOrder = (order: Order): boolean =>
    order.type === OrderType.StopLoss ||
    order.type === OrderType.TakeProfit ||
    Boolean((order.info as Record<string, any> | undefined)?.__mexcPlanOrder) ||
    Boolean((order.info as Record<string, any> | undefined)?.__mexcStopOrder);

  private isStopOrder = (order: Order): boolean =>
    Boolean((order.info as Record<string, any> | undefined)?.__mexcStopOrder) ||
    ((order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit) &&
      order.amount === 0 &&
      order.remaining === 0);

  private isPlanOrder = (order: Order): boolean => this.isStopLikeOrder(order) && !this.isStopOrder(order);

  private noteRecentlyCanceledStopLikeOrders = (orders: Order[]): void => {
    const now = virtualClock.getCurrentTimeMs();
    for (const order of orders) {
      const cleanId = order.id.split('__')[0];
      this.recentlyCanceledStopLikeOrderIds.set(cleanId, now);
      // Whether the venue already surfaced the row or not, a cancel is a
      // stronger signal than "we just placed it" — drop the placement shield
      // entry so the row is not resurrected by a mid-flight snapshot.
      this.recentlyPlacedStopLikeOrderIds.delete(cleanId);
    }
  };

  private isRecentlyCanceledStopLikeOrder = (order: Order): boolean =>
    this.isRecentlyCanceledStopLikeOrderId(order.id);

  // Public shield-check for the private WS handlers. WS frames don't carry a
  // full `Order`, so `handlePlanOrder` / `handleStopOrder` need an id-based
  // variant to decide whether a lagging Open frame is stale (already locally
  // canceled) and should be dropped instead of resurrecting the row.
  isRecentlyCanceledStopLikeOrderId = (orderId: string): boolean => {
    if (!orderId) return false;
    const id = orderId.split('__')[0];
    const canceledAt = this.recentlyCanceledStopLikeOrderIds.get(id);
    if (canceledAt === undefined) return false;

    if (virtualClock.getCurrentTimeMs() - canceledAt > MEXC_RECENT_STOP_CANCEL_SHIELD_MS) {
      this.recentlyCanceledStopLikeOrderIds.delete(id);
      return false;
    }
    return true;
  };

  private filterRecentlyCanceledStopLikeOrders = (orders: Order[]): Order[] =>
    orders.filter((order) => !this.isStopLikeOrder(order) || !this.isRecentlyCanceledStopLikeOrder(order));

  // Called from `MexcPrivateWebsocket.handleOrder` when a `push.personal.order`
  // frame reports the venue itself canceled a just-placed order that never
  // filled (`state=4`, `errorCode>0`, `dealVol=0`). Registers the id in a
  // short shield window so the optimistic-add and preservation paths don't
  // resurrect the row, and surfaces a user-visible error explaining why
  // the order didn't stick.
  noteRejectedRegularOrder = (orderId: string, errorCode: number): void => {
    if (!orderId) return;
    const cleanId = orderId.split('__')[0];
    this.recentlyRejectedRegularOrderIds.set(cleanId, virtualClock.getCurrentTimeMs());

    const existing = this.store.orders.find((order) => order.id === cleanId);
    if (existing && existing.status !== OrderStatus.Canceled && existing.status !== OrderStatus.Closed) {
      this.store.updateOrderStatus({ id: cleanId }, OrderStatus.Canceled);
    }

    const reason = MEXC_ORDER_CANCEL_REASONS[errorCode];
    const detail = reason ? `${reason} (errorCode ${errorCode})` : `errorCode ${errorCode}`;
    this.emitAndLogError(`MEXC canceled order ${cleanId}: ${detail}`);
  };

  private isRecentlyRejectedRegularOrder = (orderId: string): boolean => {
    const cleanId = orderId.split('__')[0];
    const rejectedAt = this.recentlyRejectedRegularOrderIds.get(cleanId);
    if (rejectedAt === undefined) return false;

    if (virtualClock.getCurrentTimeMs() - rejectedAt > MEXC_RECENT_REJECTED_ORDER_SHIELD_MS) {
      this.recentlyRejectedRegularOrderIds.delete(cleanId);
      return false;
    }
    return true;
  };

  private assertCommonSuccess = (payload: any, action: string): void => {
    const code = payload?.code === undefined || payload?.code === null ? undefined : toNumber(payload.code, Number.NaN);
    if (payload && payload.success !== false && code === 0) return;

    throw new Error(`${action} failed: ${payload?.message || payload?.msg || payload?.code || 'unknown error'}`);
  };

  private assertRegularCancelSucceeded = (payload: any): void => {
    this.assertCommonSuccess(payload, 'MEXC order cancellation');
    const failedRows = extractRows<Record<string, any>>(payload).filter((row) => toNumber(row.errorCode, 0) !== 0);
    if (!failedRows.length) return;

    const message = failedRows
      .map((row) => `${toIntegerString(row.orderId) || 'unknown'}: ${row.errorMsg || row.message || row.msg || row.errorCode}`)
      .join(', ');
    throw new Error(`MEXC order cancellation failed: ${message}`);
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    const market = this.store.marketsBySymbol[symbol];
    for (const [endpoint, accepts] of [
      [ENDPOINTS.ORDER_CANCEL_ALL, (order: Order) => !this.isStopLikeOrder(order)],
      ['/api/v1/private/planorder/cancel_all', (order: Order) => this.isPlanOrder(order)],
      ['/api/v1/private/stoporder/cancel_all', (order: Order) => this.isStopOrder(order)],
    ] as const) {
      const { data } = await this.xhr.post(endpoint, { symbol: market?.id || toMexcSymbol(symbol) });
      this.assertCommonSuccess(data, 'MEXC cancel-all');
      const orders = this.store.orders.filter((order) => order.symbol === symbol && accepts(order) && order.status !== OrderStatus.Closed);
      this.noteRecentlyCanceledStopLikeOrders(orders.filter((order) => this.isStopLikeOrder(order)));
      this.store.markOrdersTerminal(orders.map((order) => ({ id: order.id })), OrderStatus.Canceled);
    }
  };

  cancelAllOrders = async (): Promise<void> => {
    await this.xhr.post(ENDPOINTS.ORDER_CANCEL_ALL, {});
    this.store.updateOrdersStatus(
      this.store.orders.map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    const market = this.store.marketsBySymbol[symbol];
    const config = this.symbolSettings.get(symbol) || this.deriveSymbolConfig(symbol);

    try {
      if (market && !this.isPublicDataOnly) {
        const { data } = await this.xhr.get(ENDPOINTS.POSITION_LEVERAGE, {
          params: { symbol: market.id },
        });
        const rows = extractRows<Record<string, any>>(data);
        const longRow = rows.find((row) => toNumber(row.positionType, Number.NaN) === 1);
        const shortRow = rows.find((row) => toNumber(row.positionType, Number.NaN) === 2);
        const buyLeverage = toNumber(longRow?.leverage, config.buyLeverage);
        const sellLeverage = toNumber(shortRow?.leverage, config.sellLeverage);
        const maxLeverageCandidates = rows
          .map((row) => toNumber(row.maxLeverageView ?? row.maxLeverage, Number.NaN))
          .filter(Number.isFinite);
        const maxLeverage = maxLeverageCandidates.length ? Math.min(...maxLeverageCandidates) : undefined;
        const marginType = normalizeMarginType(longRow?.openType ?? shortRow?.openType ?? REVERSE_MARGIN_TYPE[config.marginType]);
        const nextConfig = {
          ...config,
          buyLeverage,
          sellLeverage,
          marginType,
        };

        if (maxLeverage && maxLeverage > 0 && maxLeverage !== market.limits.leverage.max) {
          this.store.updateMarket(
            { id: market.id },
            {
              limits: {
                ...market.limits,
                leverage: {
                  ...market.limits.leverage,
                  max: maxLeverage,
                },
              },
            },
          );
        }

        this.symbolSettings.set(symbol, nextConfig);
        const fetchedLeverage = {
          buyLeverage,
          sellLeverage,
          marginType,
          positionMode: config.positionMode,
        };
        this.store.update({
          fetchedLeverage: {
            ...this.store.fetchedLeverage,
            [symbol]: fetchedLeverage,
          },
        });
        return fetchedLeverage;
      }
    } catch (err: any) {
      this.log(`Failed to fetch MEXC leverage cap for ${symbol}: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
    }

    const fetchedLeverage = {
      buyLeverage: config.buyLeverage,
      sellLeverage: config.sellLeverage,
      marginType: config.marginType,
      positionMode: config.positionMode,
    };
    this.store.update({
      fetchedLeverage: {
        ...this.store.fetchedLeverage,
        [symbol]: fetchedLeverage,
      },
    });
    return fetchedLeverage;
  };

  updateSymbolMetadata = async (symbol: string) => {
    await this.fetchLeverageForSymbol(symbol);
  };

  setLeverage = async (symbol: string, leverage: number, side?: PositionSide): Promise<void> => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    const nextLeverage = Math.min(Math.max(leverage, market.limits.leverage.min), market.limits.leverage.max);
    const currentConfig = this.symbolSettings.get(symbol) || this.deriveSymbolConfig(symbol);
    const positionTypes: Array<1 | 2> = side === PositionSide.Short ? [2] : side === PositionSide.Long ? [1] : [1, 2];
    for (const positionType of positionTypes) {
      const { data } = await this.xhr.post(
        ENDPOINTS.CHANGE_LEVERAGE,
        this.formatChangeLeveragePayload(symbol, market, positionType, nextLeverage, currentConfig.marginType),
      );
      this.assertCommonSuccess(data, 'MEXC leverage change');
    }

    const nextConfig = {
      ...currentConfig,
      buyLeverage: side === PositionSide.Short ? currentConfig.buyLeverage : nextLeverage,
      sellLeverage: side === PositionSide.Long ? currentConfig.sellLeverage : nextLeverage,
    };
    this.symbolSettings.set(symbol, nextConfig);
    this.updateFetchedLeverage(symbol, nextConfig);
  };

  changePositionMode = async (hedged: boolean): Promise<void> => {
    const mode = hedged ? PositionMode.Hedged : PositionMode.OneWay;
    await this.xhr.post(ENDPOINTS.CHANGE_POSITION_MODE, {
      positionMode: REVERSE_POSITION_MODE[mode],
    });
    this.accountPositionMode = mode;
    for (const [symbol, config] of this.symbolSettings) {
      const nextConfig = { ...config, positionMode: mode };
      this.symbolSettings.set(symbol, nextConfig);
      this.updateFetchedLeverage(symbol, nextConfig);
    }
    this.store.setSetting('isHedged', hedged);
    const positions = await this.fetchPositions();
    this.store.addOrUpdatePositionsFull(positions);
  };

  changeMarginType = async (isolated: boolean, symbol?: string): Promise<void> => {
    if (!symbol) throw new Error('Symbol is required');
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    const marginType = isolated ? MarginType.Isolated : MarginType.Cross;
    const currentConfig = this.symbolSettings.get(symbol) || this.deriveSymbolConfig(symbol);
    const leverageByPositionType = {
      1: Math.min(Math.max(currentConfig.buyLeverage || 20, market.limits.leverage.min), market.limits.leverage.max),
      2: Math.min(Math.max(currentConfig.sellLeverage || 20, market.limits.leverage.min), market.limits.leverage.max),
    };
    for (const positionType of [1, 2] as const) {
      const { data } = await this.xhr.post(
        ENDPOINTS.CHANGE_LEVERAGE,
        this.formatChangeLeveragePayload(symbol, market, positionType, leverageByPositionType[positionType], marginType, {
          includeOpenTypeWithPositionId: true,
        }),
      );
      this.assertCommonSuccess(data, 'MEXC margin type change');
    }
    const nextConfig = {
      ...currentConfig,
      marginType,
    };
    this.symbolSettings.set(symbol, nextConfig);
    this.updateFetchedLeverage(symbol, nextConfig);
  };

  private updateFetchedLeverage = (symbol: string, config: MexcSymbolConfig): void => {
    this.store.update({
      fetchedLeverage: {
        ...this.store.fetchedLeverage,
        [symbol]: {
          buyLeverage: config.buyLeverage,
          sellLeverage: config.sellLeverage,
          marginType: config.marginType,
          positionMode: config.positionMode,
        },
      },
    });
  };

  private formatChangeLeveragePayload = (
    symbol: string,
    market: Market,
    positionType: 1 | 2,
    leverage: number,
    marginType: MarginType,
    options: { includeOpenTypeWithPositionId?: boolean } = {},
  ): Record<string, any> => {
    const positionSide = positionType === 1 ? PositionSide.Long : PositionSide.Short;
    const position = this.findOpenPosition(symbol, positionSide, this.store.positions);
    const positionId = position ? this.nativePositionId(position) : '';
    const openType = REVERSE_MARGIN_TYPE[marginType];

    if (positionId) {
      return omitUndefined({
        positionId,
        leverage,
        openType: options.includeOpenTypeWithPositionId ? openType : undefined,
      });
    }

    return {
      symbol: market.id,
      positionType,
      leverage,
      openType,
    };
  };

  private deriveSymbolConfig = (symbol: string): MexcSymbolConfig => {
    const existingConfig = this.symbolSettings.get(symbol);
    const positions = this.store.positions.filter((item) => item.symbol === symbol && item.contracts);
    const position = positions[0];
    const positionMode = position?.positionMode || existingConfig?.positionMode || this.accountPositionMode;
    const longPosition = positions.find((item) => item.side === PositionSide.Long);
    const shortPosition = positions.find((item) => item.side === PositionSide.Short);
    const oneWayPositionLeverage = positionMode === PositionMode.OneWay && position ? toNumber(position.leverage, 20) : undefined;
    return {
      marginType: normalizeMarginType(position?.info?.openType ?? position?.marginType ?? existingConfig?.marginType),
      buyLeverage:
        longPosition !== undefined
          ? toNumber(longPosition.leverage, existingConfig?.buyLeverage ?? 20)
          : (oneWayPositionLeverage ?? existingConfig?.buyLeverage ?? existingConfig?.sellLeverage ?? 20),
      sellLeverage:
        shortPosition !== undefined
          ? toNumber(shortPosition.leverage, existingConfig?.sellLeverage ?? 20)
          : (oneWayPositionLeverage ?? existingConfig?.sellLeverage ?? existingConfig?.buyLeverage ?? 20),
      positionMode,
    };
  };

  private ensureSymbolConfig = async (symbol: string): Promise<MexcSymbolConfig> => {
    const existingConfig = this.symbolSettings.get(symbol);
    if (existingConfig) return existingConfig;

    if (!this.isPublicDataOnly) {
      await this.fetchLeverageForSymbol(symbol);
      const fetchedConfig = this.symbolSettings.get(symbol);
      if (fetchedConfig) return fetchedConfig;
    }

    return this.deriveSymbolConfig(symbol);
  };

  private getLeverageForOrder = (opts: PlaceOrderOpts): number => {
    const config = this.symbolSettings.get(opts.symbol) || this.deriveSymbolConfig(opts.symbol);
    if (opts.reduceOnly) {
      return opts.side === OrderSide.Sell ? config.buyLeverage : config.sellLeverage;
    }
    return opts.side === OrderSide.Buy ? config.buyLeverage : config.sellLeverage;
  };

  private orderListEndpoint = (venueSymbol?: string): string =>
    venueSymbol ? `${ENDPOINTS.OPEN_ORDERS}/${venueSymbol}` : ENDPOINTS.OPEN_ORDERS;

  override dispose(): void {
    // Cancel any in-flight coalesced TP/SL refresh timers so a delayed
    // `fetchAndSetOrders` doesn't fire after the sockets have been torn down
    // and leak an XHR against a disposed exchange.
    for (const timer of this.pendingStopSnapshotRefreshTimers.values()) {
      clearTimeout(timer);
    }
    this.pendingStopSnapshotRefreshTimers.clear();
    super.dispose();
  }
}
