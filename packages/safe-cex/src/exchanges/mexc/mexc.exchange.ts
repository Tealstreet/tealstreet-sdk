import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';

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
} from '../../types';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust } from '../../utils/safe-math';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
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
const MEXC_REGULAR_ORDER_THROTTLE_COOLDOWN_MS = 1_500;

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
      const orders = await this.fetchOrders();
      if (this.isDisposed) return;
      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });
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
          config ? Math.min(config.buyLeverage, config.sellLeverage) : 20,
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

    return uniqBy(orders, 'id');
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
        orders.push(
          ...extractRows<Record<string, any>>(data, ['orders', 'items'])
            .map((row) => normalizeOrder(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
            .filter((order): order is Order => !!order),
        );
      } catch (err: any) {
        this.log(`Failed to fetch MEXC open orders: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
      }
    }

    return uniqBy(orders, 'id');
  };

  fetchAndSetRegularOrders = async (symbols?: string[]): Promise<Order[]> => {
    const regularOrders = await this.fetchRegularOrders(symbols);

    if (symbols?.length) {
      for (const symbol of symbols) {
        const symbolRegularOrders = regularOrders.filter((order) => order.symbol === symbol);
        const symbolStopLikeOrders = this.store.orders.filter(
          (order) => order.symbol === symbol && this.isStopLikeOrder(order),
        );
        this.store.setOrdersForSymbol(symbol, uniqBy([...symbolStopLikeOrders, ...symbolRegularOrders], 'id'));
      }
    } else {
      const stopLikeOrders = this.store.orders.filter((order) => this.isStopLikeOrder(order));
      this.store.update({
        orders: uniqBy([...stopLikeOrders, ...regularOrders], 'id'),
        loaded: { ...this.store.loaded, orders: true },
      });
    }

    return regularOrders;
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
    return {
      ...normalizeOrderBook(data, market),
      lastUpdateId: toIntegerString(extractData(data)?.version),
    };
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    if (opts.type === OrderType.TrailingStopLoss) {
      const message = 'MEXC trailing-stop order placement is not implemented yet';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) {
      return [await this.placePlanOrder(opts)];
    }

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
    const payload = omitUndefined({
      symbol: market.id,
      price: opts.type === OrderType.Market ? undefined : `${adjust(opts.price || 0, market.precision.price)}`,
      vol,
      side,
      type,
      openType: REVERSE_MARGIN_TYPE[this.symbolSettings.get(opts.symbol)?.marginType || MarginType.Cross],
      leverage: Math.min(Math.max(leverage, market.limits.leverage.min), market.limits.leverage.max),
      externalOid: `ts_${uuid()}`,
      reduceOnly: opts.reduceOnly || undefined,
    });

    try {
      const { data } = await this.enqueueRegularOrderSubmit(() => this.xhr.post(ENDPOINTS.ORDER_SUBMIT, payload));
      this.assertRegularOrderSubmitSucceeded(data);
      const orderId = toIntegerString(extractData(data)?.orderId ?? extractData(data));
      if (!orderId) throw new Error('MEXC order placement returned no order id');
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

    const leverage = Math.min(Math.max(this.getLeverageForOrder(opts), market.limits.leverage.min), market.limits.leverage.max);
    const config = this.symbolSettings.get(opts.symbol) || this.deriveSymbolConfig(opts.symbol);
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

  private addOptimisticStopOrder = (
    opts: PlaceOrderOpts,
    market: Market,
    orderId: string,
    triggerPrice: number,
    marker: '__mexcPlanOrder' | '__mexcStopOrder',
  ): void => {
    const nativeAmount = toNumber(opts.amount, 0);
    const amount = nativeAmount === 0 ? 0 : nativeVolumeToAmount(nativeAmount, market);
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
  };

  private scheduleStopOrderSnapshotRefresh = (symbol: string): void => {
    const timeout = setTimeout(() => {
      void this.fetchAndSetOrders([symbol]).catch((err: any) => {
        this.log(`Failed to refresh MEXC TP/SL orders: ${err?.response?.data?.message || err?.message}`, LogSeverity.Warning);
      });
    }, 1_500);
    (timeout as ReturnType<typeof setTimeout> & { unref?: () => void }).unref?.();
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
    const triggerType = opts.type === OrderType.TakeProfit ? opts.tpTriggerType : opts.slTriggerType;
    if (triggerType === OrderStopTriggerType.MarkPrice) return 2;
    if (triggerType === OrderStopTriggerType.IndexPrice) return 3;
    return MEXC_TRIGGER_TREND_LATEST_PRICE;
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    const ids: string[] = [];
    for (let index = 0; index < orders.length; index += 1) {
      try {
        ids.push(...(await this.placeOrder(orders[index])));
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

  updateOrder = async ({ order, update }: UpdateOrderOpts): Promise<string[]> => {
    const nextOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      amount: 'amount' in update ? update.amount : this.getReplacementOrderAmount(order),
      price: 'price' in update ? update.price : order.price,
      reduceOnly: order.reduceOnly,
      timeInForce: order.timeInForce,
    };

    await this.cancelOrders([order]);
    return await this.placeOrder(nextOrder);
  };

  private getReplacementOrderAmount = (order: Order): number => {
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

    for (const lot of chunk(stopOrders, 50)) {
      await this.xhr.post(
        ENDPOINTS.STOP_ORDER_CANCEL,
        lot.map((order) => ({ stopPlanOrderId: order.id.split('__')[0] })),
      );
    }

    for (const lot of chunk(planOrders, 50)) {
      await this.xhr.post(
        ENDPOINTS.PLAN_ORDER_CANCEL,
        lot.map((order) => ({ symbol: toMexcSymbol(order.symbol), orderId: order.id.split('__')[0] })),
      );
    }

    for (const lot of chunk(regularOrders, 50)) {
      const { data } = await this.xhr.post(
        ENDPOINTS.ORDER_CANCEL,
        lot.map((order) => order.id.split('__')[0]),
      );
      this.assertRegularCancelSucceeded(data);
    }

    this.store.updateOrdersStatus(
      orders.map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
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

  private assertRegularCancelSucceeded = (payload: any): void => {
    const failedRows = extractRows<Record<string, any>>(payload).filter((row) => toNumber(row.errorCode, 0) !== 0);
    if (!failedRows.length) return;

    const message = failedRows
      .map((row) => `${toIntegerString(row.orderId) || 'unknown'}: ${row.errorMsg || row.message || row.msg || row.errorCode}`)
      .join(', ');
    throw new Error(`MEXC order cancellation failed: ${message}`);
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    const market = this.store.marketsBySymbol[symbol];
    await this.xhr.post(ENDPOINTS.ORDER_CANCEL_ALL, omitUndefined({ symbol: market?.id || toMexcSymbol(symbol) }));
    this.store.updateOrdersStatus(
      this.store.orders.filter((order) => order.symbol === symbol).map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
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
    const positionTypes = side === PositionSide.Short ? [2] : side === PositionSide.Long ? [1] : [1, 2];
    for (const positionType of positionTypes) {
      await this.xhr.post(ENDPOINTS.CHANGE_LEVERAGE, {
        symbol: market.id,
        positionType,
        leverage: nextLeverage,
        openType: REVERSE_MARGIN_TYPE[currentConfig.marginType],
      });
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
      await this.xhr.post(ENDPOINTS.CHANGE_LEVERAGE, {
        symbol: market.id,
        positionType,
        leverage: leverageByPositionType[positionType],
        openType: REVERSE_MARGIN_TYPE[marginType],
      });
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

  private getLeverageForOrder = (opts: PlaceOrderOpts): number => {
    const config = this.symbolSettings.get(opts.symbol) || this.deriveSymbolConfig(opts.symbol);
    if (opts.reduceOnly) {
      return opts.side === OrderSide.Sell ? config.buyLeverage : config.sellLeverage;
    }
    return opts.side === OrderSide.Buy ? config.buyLeverage : config.sellLeverage;
  };

  private orderListEndpoint = (venueSymbol?: string): string =>
    venueSymbol ? `${ENDPOINTS.OPEN_ORDERS}/${venueSymbol}` : ENDPOINTS.OPEN_ORDERS;
}
