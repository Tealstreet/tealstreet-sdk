import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';

import createHash from 'create-hash';
import chunk from 'lodash/chunk';
import uniqBy from 'lodash/uniqBy';

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
  normalizeOrder,
  normalizeOrderBook,
  normalizePlanOrder,
  normalizePosition,
  normalizePositionMode,
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
  leverage: number;
  positionMode: PositionMode;
};

const MEXC_TRIGGER_GTE = 1;
const MEXC_TRIGGER_LTE = 2;
const MEXC_EXECUTE_CYCLE_SEVEN_DAYS = 2;
const MEXC_TRIGGER_TREND_LATEST_PRICE = 1;
const MEXC_PLAN_ORDER_LOOKBACK_MS = 89 * 24 * 60 * 60 * 1000;

export class MexcExchange extends BaseExchange {
  xhr: Axios;
  publicWebsocket: MexcPublicWebsocket;
  privateWebsocket: MexcPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private accountPositionMode: PositionMode = PositionMode.Hedged;
  private symbolSettings = new Map<string, MexcSymbolConfig>();

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
          config?.leverage || 20,
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
    }

    return uniqBy(orders, 'id');
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
    const leverage = this.symbolSettings.get(opts.symbol)?.leverage || this.findPositionLeverage(opts.symbol) || 20;
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
      const { data } = await this.xhr.post(ENDPOINTS.ORDER_SUBMIT, payload);
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

    const config = this.symbolSettings.get(opts.symbol) || this.deriveSymbolConfig(opts.symbol);
    const leverage = Math.min(Math.max(config.leverage || 20, market.limits.leverage.min), market.limits.leverage.max);
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
      trend: MEXC_TRIGGER_TREND_LATEST_PRICE,
      positionMode: REVERSE_POSITION_MODE[config.positionMode],
      reduceOnly: true,
    });

    try {
      const { data } = await this.xhr.post(ENDPOINTS.PLAN_ORDER_PLACE, payload);
      const orderId = toIntegerString(extractData(data)?.orderId ?? extractData(data));
      if (!orderId) throw new Error('MEXC plan order placement returned no order id');
      return orderId;
    } catch (err: any) {
      const message = err?.response?.data?.message || err?.response?.data?.msg || err?.message || 'MEXC plan order placement failed';
      this.emitAndLogError(message);
      throw err;
    }
  };

  private planOrderTriggerType = (opts: PlaceOrderOpts): number => {
    const closesLong = opts.side === OrderSide.Sell;
    if (opts.type === OrderType.TakeProfit) {
      return closesLong ? MEXC_TRIGGER_GTE : MEXC_TRIGGER_LTE;
    }
    return closesLong ? MEXC_TRIGGER_LTE : MEXC_TRIGGER_GTE;
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    const ids: string[] = [];
    for (const order of orders) {
      ids.push(...(await this.placeOrder(order)));
    }
    return ids;
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts): Promise<string[]> => {
    const nextOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      amount: 'amount' in update ? update.amount : order.amount,
      price: 'price' in update ? update.price : order.price,
      reduceOnly: order.reduceOnly,
      timeInForce: order.timeInForce,
    };

    await this.cancelOrders([order]);
    return await this.placeOrder(nextOrder);
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    if (!orders.length) return;

    const planOrders = orders.filter((order) => this.isPlanOrder(order));
    const regularOrders = orders.filter((order) => !this.isPlanOrder(order));

    for (const lot of chunk(planOrders, 50)) {
      await this.xhr.post(
        ENDPOINTS.PLAN_ORDER_CANCEL,
        lot.map((order) => ({ symbol: toMexcSymbol(order.symbol), orderId: order.id.split('__')[0] })),
      );
    }

    for (const lot of chunk(regularOrders, 50)) {
      await this.xhr.post(
        ENDPOINTS.ORDER_CANCEL,
        lot.map((order) => ({ orderId: order.id.split('__')[0] })),
      );
    }

    this.store.updateOrdersStatus(
      orders.map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
  };

  private isPlanOrder = (order: Order): boolean =>
    order.type === OrderType.StopLoss ||
    order.type === OrderType.TakeProfit ||
    Boolean((order.info as Record<string, any> | undefined)?.__mexcPlanOrder);

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
    const config = this.symbolSettings.get(symbol) || this.deriveSymbolConfig(symbol);
    const fetchedLeverage = {
      buyLeverage: config.leverage,
      sellLeverage: config.leverage,
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
    this.symbolSettings.set(symbol, this.deriveSymbolConfig(symbol));
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
      leverage: nextLeverage,
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
    const leverage = Math.min(Math.max(currentConfig.leverage || 20, market.limits.leverage.min), market.limits.leverage.max);
    for (const positionType of [1, 2]) {
      await this.xhr.post(ENDPOINTS.CHANGE_LEVERAGE, {
        symbol: market.id,
        positionType,
        leverage,
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
          buyLeverage: config.leverage,
          sellLeverage: config.leverage,
          marginType: config.marginType,
          positionMode: config.positionMode,
        },
      },
    });
  };

  private deriveSymbolConfig = (symbol: string): MexcSymbolConfig => {
    const existingConfig = this.symbolSettings.get(symbol);
    const position = this.store.positions.find((item) => item.symbol === symbol && item.contracts);
    return {
      marginType: normalizeMarginType(position?.info?.openType ?? position?.marginType ?? existingConfig?.marginType),
      leverage: position ? toNumber(position.leverage, existingConfig?.leverage ?? 20) : (existingConfig?.leverage ?? 20),
      positionMode: position?.positionMode || existingConfig?.positionMode || this.accountPositionMode,
    };
  };

  private findPositionLeverage = (symbol: string): number => {
    const position = this.store.positions.find((item) => item.symbol === symbol && item.contracts);
    return toNumber(position?.leverage, 0);
  };

  private orderListEndpoint = (venueSymbol?: string): string =>
    venueSymbol ? `${ENDPOINTS.OPEN_ORDERS}/${venueSymbol}` : ENDPOINTS.OPEN_ORDERS;
}
