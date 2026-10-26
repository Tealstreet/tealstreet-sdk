import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';

import createHash from 'create-hash';
import chunk from 'lodash/chunk';
import uniq from 'lodash/uniq';
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
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
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
  PublicTrade,
  Ticker,
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { paginateIncomeRange } from '../../../exchanges/income-paging';
import { createAPI } from './weex.api';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  REVERSE_MARGIN_TYPE,
  REVERSE_ORDER_SIDE,
  REVERSE_ORDER_TRIGGER_TYPE,
  REVERSE_ORDER_TYPE,
  REVERSE_POSITION_MODE,
  REVERSE_POSITION_SIDE,
  REVERSE_TIME_IN_FORCE,
  SUPPORTED_INTERVALS,
  WeexExchangeInfoResponse,
  WeexKlineRow,
  WeexOrderResult,
} from './weex.types';
import {
  fakePositionsForMarket,
  normalizeAlgoOrder,
  normalizeBalance,
  normalizeCandle,
  normalizeExecution,
  normalizeIncome,
  normalizeMarket,
  normalizeOrder,
  normalizeOrderBook,
  normalizePosition,
  normalizePublicTrade,
  normalizeTicker,
  normalizeTimestampToMs,
  toIntegerString,
  toNumber,
} from './weex.utils';
import { WeexPrivateWebsocket } from './weex.ws-private';
import { WeexPublicWebsocket } from './weex.ws-public';

type SymbolConfig = {
  marginType: MarginType;
  positionMode: PositionMode;
  crossLeverage: number;
  isolatedLongLeverage: number;
  isolatedShortLeverage: number;
};

/**
 * Widest income window accepted in one request, and the venue's own row cap.
 *
 * Both are UNVERIFIED against the live venue — WEEX has no credential anywhere in the
 * `yarn keys` sheet. The 100-day window is carried over from the previous implementation;
 * the 100-row cap is what `fetchIncomeHistory` already clamps to. They are named here rather
 * than left inline so the pager's correctness does not depend on either being right: it
 * pages until the records span the window, so a window that is actually narrower costs extra
 * requests instead of silently losing rows.
 */
const WEEX_INCOME_MAX_WINDOW_MS = 100 * 24 * 60 * 60 * 1000;
const WEEX_INCOME_PAGE_LIMIT = 100;

const extractArray = <T = Record<string, any>>(payload: any, keys: string[] = []): T[] => {
  if (Array.isArray(payload)) return payload;
  if (Array.isArray(payload?.data)) return payload.data;
  for (const key of keys) {
    if (Array.isArray(payload?.[key])) return payload[key];
    if (Array.isArray(payload?.data?.[key])) return payload.data[key];
  }
  return [];
};

export class WeexExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('WEEX', /^[.A-Z:/a-z0-9_-]{1,36}$/, {
    mint: () => `ts${uuid()}`.slice(0, 36),
  });

  xhr: Axios;
  publicWebsocket: WeexPublicWebsocket;
  privateWebsocket: WeexPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private symbolSettings = new Map<string, SymbolConfig>();
  private accountPositionMode: PositionMode = PositionMode.OneWay;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI);
    this.publicWebsocket = new WeexPublicWebsocket(this);
    this.privateWebsocket = new WeexPrivateWebsocket(this);
  }

  validateAccount = async () => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_CONFIG);
      if (data?.canTrade === false) {
        return { error: 'WEEX API key is valid but trading is disabled' };
      }
      return {
        error: '',
        balance: await this.fetchBalanceAndPositions()
          .then((r) => r.balance)
          .catch(() => undefined),
      };
    } catch (err: any) {
      const message =
        err?.response?.data?.msg || err?.response?.data?.message || err?.message || 'Invalid API key or secret';
      this.emitAndLogError(message);
      return { error: message };
    }
  };

  getAccount = async (): Promise<ExchangeAccount> => {
    const validationError = await this.validateAccount();
    if (validationError.error) {
      throw new Error(validationError.error);
    }

    const userId = createHash('sha256')
      .update(this.options.key || 'weex')
      .digest('hex')
      .slice(0, 24);
    return { userId: `weex:${userId}` };
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
      this.log(`Loaded ${Math.min(markets.length, tickers.length)} WEEX markets`);
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_CONFIG);
      this.accountPositionMode = data?.dualSidePosition ? PositionMode.Hedged : PositionMode.OneWay;
      this.store.setSetting('isHedged', this.accountPositionMode === PositionMode.Hedged);
    } catch (err: any) {
      this.log(`Failed to load WEEX account config: ${err?.response?.data?.msg || err?.message}`, LogSeverity.Warning);
    }

    await this.fetchSymbolConfigs();
    if (this.isDisposed) return;

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
    this.log('Ready to trade on WEEX');
  };

  tick = async () => {
    if (this.isDisposed || this.isPublicDataOnly) {
      return;
    }

    try {
      const { balance, positions } = await this.fetchBalanceAndPositions();
      if (this.isDisposed) return;
      this.store.addOrUpdatePositionsFull(positions);
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true, positions: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }

    try {
      const orders = await this.fetchOrders();
      if (this.isDisposed) return;
      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }

    if (!this.isDisposed) {
      loop(() => this.tick(), this.getTickInterval());
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const { data } = await this.xhr.get<WeexExchangeInfoResponse>(ENDPOINTS.EXCHANGE_INFO);
      return (data.symbols || [])
        .filter((symbol) => symbol.quoteAsset === 'USDT' && symbol.marginAsset === 'USDT')
        .map(normalizeMarket);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      const [tickerResponse, bookTickerResponse] = await Promise.all([
        this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.TICKER_24HR),
        this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.BOOK_TICKER),
      ]);

      const tickersBySymbol = new Map(tickerResponse.data.map((ticker) => [String(ticker.symbol), ticker]));
      const bookTickersBySymbol = new Map(bookTickerResponse.data.map((ticker) => [String(ticker.symbol), ticker]));

      return this.store.markets.reduce<Ticker[]>((acc, market) => {
        const ticker = tickersBySymbol.get(market.symbol);
        const bookTicker = bookTickersBySymbol.get(market.symbol);
        if (!ticker && !bookTicker) return acc;
        acc.push(normalizeTicker(ticker, market, bookTicker));
        return acc;
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  fetchSymbolMaxLeverage = async (symbol: string) => {
    const market = this.store.marketsBySymbol[symbol];
    if (market) {
      return market.limits.leverage.max;
    }

    const { data } = await this.xhr.get<WeexExchangeInfoResponse>(ENDPOINTS.EXCHANGE_INFO, { params: { symbol } });
    const raw = data.symbols?.[0];
    return toNumber(raw?.maxLeverage, 1);
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      const config = await this.getOrFetchSymbolConfig(symbol, true);
      const leverageObj: FetchedLeverage = {
        buyLeverage: config.marginType === MarginType.Isolated ? config.isolatedLongLeverage : config.crossLeverage,
        sellLeverage: config.marginType === MarginType.Isolated ? config.isolatedShortLeverage : config.crossLeverage,
      };
      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: leverageObj,
        },
      });
      return leverageObj;
    } catch (err: any) {
      this.log(
        `Error fetching WEEX leverage for ${symbol}: ${err?.response?.data?.msg || err?.message}`,
        LogSeverity.Warning,
      );
      return null;
    }
  };

  updateSymbolMetadata = async (symbol: string) => {
    await this.getOrFetchSymbolConfig(symbol, true);
  };

  fetchBalance = async (): Promise<Balance> => {
    const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.BALANCE);
    return normalizeBalance(extractArray(data));
  };

  fetchBalanceAndPositions = async () => {
    const [balance, positions] = await Promise.all([this.fetchBalance(), this.fetchPositions()]);
    return { balance, positions };
  };

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    try {
      const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.POSITIONS);
      const requestedSymbols = symbols?.length ? symbols : this.store.markets.map((market) => market.symbol);
      const requestedSet = new Set(requestedSymbols);

      const openPositions = extractArray(data)
        .map((raw) =>
          normalizePosition(raw, this.store.marketsBySymbol[raw.symbol], this.getSymbolConfigFromCache(raw.symbol)),
        )
        .filter((position): position is Position => !!position && requestedSet.has(position.symbol));

      const existingIds = new Set(openPositions.map((position) => position.id));
      const fakePositions = requestedSymbols.flatMap((symbol) => {
        const market = this.store.marketsBySymbol[symbol];
        if (!market) return [];
        const config = this.getSymbolConfigFromCache(symbol);
        return fakePositionsForMarket(
          market,
          config?.positionMode || this.accountPositionMode,
          config?.marginType || MarginType.Cross,
          config?.crossLeverage || config?.isolatedLongLeverage || 20,
        ).filter((position) => !existingIds.has(position.id));
      });

      return uniqBy([...openPositions, ...fakePositions], 'id');
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return symbols
        ? this.store.positions.filter((position) => symbols.includes(position.symbol))
        : this.store.positions;
    }
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const [normalResult, algoResult] = await Promise.allSettled([
      this.fetchNormalOrders(symbols),
      this.fetchAlgoOrders(symbols),
    ]);

    const normalOrders = normalResult.status === 'fulfilled' ? normalResult.value : [];
    const algoOrders = algoResult.status === 'fulfilled' ? algoResult.value : [];

    return uniqBy([...normalOrders, ...algoOrders], 'id');
  };

  fetchNormalOrders = async (symbols?: string[]) => {
    const rows = await this.fetchOpenOrderRows(ENDPOINTS.OPEN_ORDERS, symbols);
    return rows
      .map((row) => normalizeOrder(row, this.store.marketsBySymbol[row.symbol]))
      .filter((order): order is Order => !!order);
  };

  fetchAlgoOrders = async (symbols?: string[]) => {
    const rows = await this.fetchOpenOrderRows(ENDPOINTS.OPEN_ALGO_ORDERS, symbols);
    return rows.flatMap((row) => normalizeAlgoOrder(row, this.store.marketsBySymbol[row.symbol]));
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    const interval = INTERVAL[opts.interval];
    if (!SUPPORTED_INTERVALS.has(interval)) {
      throw new Error(`WEEX does not support ${opts.interval} candles`);
    }

    const historical = opts.from !== undefined || opts.to !== undefined;
    const limit = Math.min(opts.limit || (historical ? 100 : 500), historical ? 100 : 1000);
    const { data } = await this.xhr.get<WeexKlineRow[]>(historical ? ENDPOINTS.HISTORY_KLINES : ENDPOINTS.KLINES, {
      params: {
        symbol: opts.symbol,
        interval,
        startTime: opts.from,
        endTime: opts.to,
        limit,
        priceType: historical ? 'LAST' : undefined,
      },
      timeout: 5 * 60 * 1000,
    });

    return (Array.isArray(data) ? data : [])
      .map(normalizeCandle)
      .filter((candle): candle is Candle => !!candle)
      .sort((a, b) => a.timestamp - b.timestamp);
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

  fetchOrderBook = async (symbol: string, limit = 200): Promise<OrderBook & { lastUpdateId?: string }> => {
    const { data } = await this.xhr.get(ENDPOINTS.DEPTH, {
      params: {
        symbol,
        limit,
      },
    });
    return {
      ...normalizeOrderBook(data),
      lastUpdateId: toIntegerString(data?.lastUpdateId),
    };
  };

  fetchPublicTrades = async (symbol: string, limit = 100): Promise<PublicTrade[]> => {
    const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.PUBLIC_TRADES, {
      params: {
        symbol,
        limit: Math.min(Math.max(limit, 1), 1000),
      },
    });

    return (Array.isArray(data) ? data : [])
      .map((row) => normalizePublicTrade(row, symbol))
      .filter((trade): trade is PublicTrade => !!trade);
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const rows = await this.fetchUserTradeRows(opts);
    return rows
      .map((row) => normalizeExecution(row, row.symbol ? this.store.marketsBySymbol[row.symbol] : undefined))
      .filter((execution): execution is Execution => !!execution)
      .sort((a, b) => b.timestamp - a.timestamp);
  };

  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { data } = await this.xhr.post(ENDPOINTS.INCOME, {
      asset: opts?.symbol ? undefined : opts?.incomeType ? undefined : '',
      symbol: opts?.symbol || '',
      incomeType: opts?.incomeType || '',
      startTime: opts?.startTime ?? null,
      endTime: opts?.endTime ?? null,
      limit: Math.min(Math.max(opts?.limit || 100, 1), 100),
    });

    return extractArray<Record<string, any>>(data, ['items']).map(normalizeIncome);
  };

  /**
   * Income over a range, paged for real.
   *
   * This used to issue ONE request per 100-day chunk at `limit: 100` with no cursor, so an
   * active account silently lost everything past the first 100 rows of each window — and the
   * ledger identity would have reconciled against the partial result and reported PASS. The
   * walk now continues until the returned records SPAN the requested window; see
   * `weex.income.ts` for why span rather than a full-page row count, and why no sort order is
   * assumed.
   */
  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    const { records, truncated, exhaustedChunks } = await paginateIncomeRange(
      async ({ startTime: from, endTime: to, limit }) => {
        const page = await this.fetchIncomeHistory({ startTime: from, endTime: to, incomeType, limit });
        // The venue shares egress with everything else on this connection.
        await sleep(100);
        return page;
      },
      {
        startTime,
        endTime,
        chunkMs: WEEX_INCOME_MAX_WINDOW_MS,
        pageLimit: WEEX_INCOME_PAGE_LIMIT,
      },
    );

    // A partial ledger that is not announced is worse than an error: it reconciles. Callers
    // cannot see the return of a truncated walk as different from a complete one, so say so.
    if (truncated) {
      this.log(
        `Income history for ${new Date(startTime).toISOString()}..${new Date(endTime).toISOString()} is INCOMPLETE — ` +
          `the page budget ran out on ${exhaustedChunks} chunk(s). Any total derived from it is a lower bound.`,
        LogSeverity.Warning,
      );
    }

    return records;
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    this.resolveCallerClientOrderId(opts);
    const payload = this.formatOrderPayload(opts);
    const endpoint =
      opts.type === OrderType.Conditional
        ? ENDPOINTS.ALGO_ORDER
        : opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit
          ? ENDPOINTS.PLACE_TP_SL_ORDER
          : ENDPOINTS.ORDER;

    let data: WeexOrderResult | WeexOrderResult[];
    try {
      ({ data } = await this.xhr.post<WeexOrderResult | WeexOrderResult[]>(endpoint, payload));
    } catch (err: any) {
      const message =
        err?.response?.data?.errorMessage ||
        err?.response?.data?.msg ||
        err?.response?.data?.message ||
        err?.message ||
        'WEEX order placement failed';
      this.emitAndLogError(message);
      throw err;
    }
    const results = Array.isArray(data) ? data : [data];
    const ids = results.map((result) =>
      this.parseOrderResult(result, payload.newClientOrderId || payload.clientAlgoId),
    );

    if (opts.timeInForce === OrderTimeInForce.PostOnly) {
      ids.forEach((id) => this.store.updateOrder({ id }, { timeInForce: OrderTimeInForce.PostOnly }));
    }

    return ids;
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    const ids: string[] = [];
    let regularBatch: PlaceOrderOpts[] = [];

    const flushRegularBatch = async () => {
      if (!regularBatch.length) return;
      ids.push(...(await this.placeRegularOrderBatch(regularBatch)));
      regularBatch = [];
    };

    for (const order of orders) {
      if (!this.isBatchableRegularOrder(order)) {
        await flushRegularBatch();
        ids.push(...(await this.placeOrder(order)));
        continue;
      }

      regularBatch.push(order);
      if (regularBatch.length === 5) await flushRegularBatch();
    }

    await flushRegularBatch();
    return ids;
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts): Promise<string[]> => {
    if (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit) {
      if (!('price' in update)) {
        const message = 'WEEX TP/SL quantity amendment is not supported';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      if (order.id.includes('__')) {
        const message = 'WEEX virtual TP/SL amendment requires the protective order ID';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      const market = this.store.marketsBySymbol[order.symbol];
      if (!market) {
        const message = `Market ${order.symbol} not found`;
        this.emitAndLogError(message);
        throw new Error(message);
      }

      const payload = {
        orderId: order.id,
        triggerPrice: `${adjust(update.price, market.precision.price)}`,
        executePrice: `${toNumber(order.info?.executePrice, 0)}`,
        triggerPriceType:
          REVERSE_ORDER_TRIGGER_TYPE[
            order.type === OrderType.TakeProfit
              ? order.tpTriggerType || OrderStopTriggerType.LastPrice
              : order.slTriggerType || OrderStopTriggerType.MarkPrice
          ],
      };

      try {
        const { data } = await this.xhr.post<{ success?: boolean }>(ENDPOINTS.MODIFY_TP_SL_ORDER, payload);
        if (data?.success === false) {
          const message = 'WEEX TP/SL amendment rejected';
          this.emitAndLogError(message);
          throw new Error(message);
        }
      } catch (err: any) {
        if (err?.message === 'WEEX TP/SL amendment rejected') throw err;
        const message =
          err?.response?.data?.errorMessage ||
          err?.response?.data?.msg ||
          err?.response?.data?.message ||
          err?.message ||
          'WEEX TP/SL amendment failed';
        this.emitAndLogError(message);
        throw err;
      }

      this.store.updateOrder({ id: order.id }, { price: update.price });
      return [order.id];
    }

    const isConditional = order.type === OrderType.Conditional;
    const nextOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      amount: 'amount' in update ? update.amount : order.amount,
      price: isConditional
        ? toNumber(order.info?.price, 0) || undefined
        : 'price' in update
          ? update.price
          : order.price,
      triggerPrice: isConditional ? ('price' in update ? update.price : order.price) : undefined,
      reduceOnly: order.reduceOnly,
      timeInForce: order.timeInForce,
      tpTriggerType: order.tpTriggerType,
      slTriggerType: order.slTriggerType,
      takeProfit: order.takeProfit,
      stopLoss: order.stopLoss,
    };

    await this.cancelOrders([order]);
    return await this.placeOrder(nextOrder);
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    if (!orders.length) return;

    const algoOrders = orders.filter((order) => this.isAlgoOrder(order));
    const normalOrders = orders.filter((order) => !this.isAlgoOrder(order));

    for (const lot of chunk(normalOrders, 10)) {
      await this.xhr.delete(ENDPOINTS.BATCH_ORDERS, {
        data: {
          orderIdList: lot.map((order) => order.id.split('__')[0]),
        },
      });
    }

    for (const order of algoOrders) {
      await this.xhr.delete(ENDPOINTS.ALGO_ORDER, {
        params: {
          orderId: order.id.split('__')[0],
        },
      });
    }

    this.store.updateOrdersStatus(
      orders.map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    await Promise.all([
      this.xhr.delete(ENDPOINTS.CANCEL_ALL_OPEN_ORDERS, { params: { symbol } }),
      this.xhr.delete(ENDPOINTS.CANCEL_ALL_ALGO_ORDERS, { params: { symbol } }),
    ]);
    this.store.updateOrdersStatus(
      this.store.orders.filter((order) => order.symbol === symbol).map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
  };

  cancelAllOrders = async (): Promise<void> => {
    await Promise.all([
      this.xhr.delete(ENDPOINTS.CANCEL_ALL_OPEN_ORDERS),
      this.xhr.delete(ENDPOINTS.CANCEL_ALL_ALGO_ORDERS),
    ]);
    this.store.updateOrdersStatus(
      this.store.orders.map((order) => ({ id: order.id })),
      OrderStatus.Canceled,
    );
  };

  setLeverage = async (symbol: string, inputLeverage: number, side?: PositionSide): Promise<void> => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    const config = await this.getOrFetchSymbolConfig(symbol);
    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);
    const payload: Record<string, any> = {
      symbol,
      marginType: REVERSE_MARGIN_TYPE[config.marginType],
    };

    if (config.marginType === MarginType.Isolated) {
      payload.isolatedLongLeverage = `${side === PositionSide.Short ? config.isolatedLongLeverage : leverage}`;
      payload.isolatedShortLeverage = `${side === PositionSide.Long ? config.isolatedShortLeverage : leverage}`;
    } else {
      payload.crossLeverage = `${leverage}`;
    }

    await this.xhr.post(ENDPOINTS.SET_LEVERAGE, payload);
    await this.updateSymbolMetadata(symbol);
  };

  changeMarginType = async (isolated: boolean, symbol?: string): Promise<void> => {
    if (!symbol) {
      throw new Error('Symbol is required');
    }

    const config = await this.getOrFetchSymbolConfig(symbol);
    await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, {
      symbol,
      marginType: isolated ? 'ISOLATED' : 'CROSSED',
      separatedType: REVERSE_POSITION_MODE[config.positionMode],
    });
    await this.updateSymbolMetadata(symbol);
  };

  changePositionMode = async (hedged: boolean, symbol?: string): Promise<void> => {
    const symbols = symbol ? [symbol] : uniq(this.store.positions.map((position) => position.symbol));
    const targetSymbols = symbols.length ? symbols : this.store.markets.map((market) => market.symbol);

    for (const targetSymbol of targetSymbols) {
      const config = await this.getOrFetchSymbolConfig(targetSymbol);
      await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, {
        symbol: targetSymbol,
        marginType: REVERSE_MARGIN_TYPE[config.marginType],
        separatedType: hedged ? 'SEPARATED' : 'COMBINED',
      });
      this.symbolSettings.set(targetSymbol, {
        ...config,
        positionMode: hedged ? PositionMode.Hedged : PositionMode.OneWay,
      });
    }

    this.accountPositionMode = hedged ? PositionMode.Hedged : PositionMode.OneWay;
    this.store.setSetting('isHedged', hedged);
  };

  nukePositions = async (): Promise<void> => {
    await this.xhr.post(ENDPOINTS.CLOSE_POSITIONS, {});
    const positions = await this.fetchPositions();
    this.store.update({ positions });
  };

  nuke = async (): Promise<void> => {
    await this.nukePositions();
    await this.cancelAllOrders();
  };

  private getSymbolConfigFromCache = (symbol: string): SymbolConfig | undefined => {
    return this.symbolSettings.get(symbol);
  };

  private getOrFetchSymbolConfig = async (symbol: string, refetch = false): Promise<SymbolConfig> => {
    const cached = this.symbolSettings.get(symbol);
    if (cached && !refetch) {
      return cached;
    }

    const configs = await this.fetchSymbolConfigs(symbol);
    return (
      configs.get(symbol) || {
        marginType: MarginType.Cross,
        positionMode: this.accountPositionMode,
        crossLeverage: 20,
        isolatedLongLeverage: 20,
        isolatedShortLeverage: 20,
      }
    );
  };

  private fetchSymbolConfigs = async (symbol?: string): Promise<Map<string, SymbolConfig>> => {
    if (this.isPublicDataOnly) {
      return this.symbolSettings;
    }

    try {
      const { data } = await this.xhr.get(ENDPOINTS.SYMBOL_CONFIG, {
        params: symbol ? { symbol } : undefined,
      });

      for (const row of extractArray<Record<string, any>>(data)) {
        const next: SymbolConfig = {
          marginType: row.marginType === 'ISOLATED' ? MarginType.Isolated : MarginType.Cross,
          positionMode: row.separatedType === 'SEPARATED' ? PositionMode.Hedged : PositionMode.OneWay,
          crossLeverage: toNumber(row.crossLeverage, 20),
          isolatedLongLeverage: toNumber(row.isolatedLongLeverage, 20),
          isolatedShortLeverage: toNumber(row.isolatedShortLeverage, 20),
        };
        this.symbolSettings.set(row.symbol, next);
      }
    } catch (err: any) {
      this.log(`Failed to fetch WEEX symbol config: ${err?.response?.data?.msg || err?.message}`, LogSeverity.Warning);
    }

    return this.symbolSettings;
  };

  private fetchOpenOrderRows = async (endpoint: string, symbols?: string[]) => {
    const requestedSymbols = symbols?.length ? symbols : [undefined];
    const rows: Array<Record<string, any>> = [];

    for (const symbol of requestedSymbols) {
      const { data } = await this.xhr.get(endpoint, {
        params: omitUndefined({
          symbol,
          limit: 100,
          page: endpoint === ENDPOINTS.OPEN_ALGO_ORDERS ? 1 : 0,
        }),
      });
      rows.push(...extractArray<Record<string, any>>(data, ['orders', 'items']));
    }

    return rows;
  };

  private fetchUserTradeRows = async (opts: ExecutionsOptions | HistoricalTradeOptions) => {
    const limit = Math.min(Math.max(opts.limit || 100, 1), 100);
    const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.USER_TRADES, {
      params: omitUndefined({
        symbol: opts.symbol,
        orderId: opts.orderId,
        startTime: opts.from,
        endTime: opts.to,
        limit,
      }),
    });

    return extractArray<Record<string, any>>(data).filter((row) => {
      if (opts.orderId && toIntegerString(row.orderId) !== toIntegerString(opts.orderId)) return false;
      const timestamp = normalizeTimestampToMs(row.time ?? row.createdTime);
      if (opts.from && timestamp < opts.from) return false;
      if (opts.to && timestamp > opts.to) return false;
      return true;
    });
  };

  private parseOrderResult = (result: WeexOrderResult, fallbackId?: string): string => {
    if (result?.success === false) {
      const message = result.errorMessage || result.errorCode || 'WEEX order rejected';
      this.emitAndLogError(message);
      throw new Error(message);
    }
    const id = toIntegerString(result?.orderId || result?.clientOrderId || fallbackId || uuid());
    return id;
  };

  private isBatchableRegularOrder = (opts: PlaceOrderOpts): boolean =>
    (opts.type === OrderType.Limit || opts.type === OrderType.Market) &&
    opts.triggerPrice === undefined &&
    opts.stopLoss === undefined &&
    opts.takeProfit === undefined &&
    opts.reduceOnly !== true &&
    opts.close !== true &&
    opts.tpTriggerType === undefined &&
    opts.slTriggerType === undefined;

  private placeRegularOrderBatch = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    const payloads = orders.map((order) => this.formatOrderPayload(order));
    const clientOrderIds: string[] = payloads.map((payload) => payload.newClientOrderId);

    if (
      clientOrderIds.some((clientOrderId) => typeof clientOrderId !== 'string' || !clientOrderId) ||
      new Set(clientOrderIds).size !== clientOrderIds.length
    ) {
      const message = 'WEEX batch order request has missing or duplicate client order IDs';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    let data: unknown;
    try {
      ({ data } = await this.xhr.post(ENDPOINTS.BATCH_ORDERS, { batchOrders: payloads }));
    } catch (err: any) {
      const message =
        err?.response?.data?.errorMessage ||
        err?.response?.data?.msg ||
        err?.response?.data?.message ||
        err?.message ||
        'WEEX batch order placement failed';
      this.emitAndLogError(message);
      throw err;
    }

    const malformed = (reason: string): never => {
      const message = `WEEX batch order response malformed: ${reason}`;
      this.emitAndLogError(message);
      throw new Error(message);
    };

    const responseRows = Array.isArray(data) ? data : malformed('expected an array');
    if (responseRows.length !== payloads.length) {
      malformed(`expected ${payloads.length} rows, received ${responseRows.length}`);
    }

    const resultsByClientOrderId = new Map<string, WeexOrderResult>();
    for (const result of responseRows) {
      if (!result || typeof result !== 'object' || Array.isArray(result)) {
        malformed('each row must be an object');
      }

      const row = result as WeexOrderResult;
      const clientOrderId =
        typeof row.clientOrderId === 'string' && row.clientOrderId
          ? row.clientOrderId
          : malformed('each row must echo a clientOrderId');
      if (!clientOrderIds.includes(clientOrderId)) {
        malformed(`unknown clientOrderId ${clientOrderId}`);
      }
      if (resultsByClientOrderId.has(clientOrderId)) {
        malformed(`duplicate clientOrderId ${clientOrderId}`);
      }
      if (typeof row.success !== 'boolean') {
        malformed(`missing success flag for ${clientOrderId}`);
      }
      if (row.errorCode !== undefined && typeof row.errorCode !== 'string') {
        malformed(`invalid errorCode for ${clientOrderId}`);
      }
      if (row.errorMessage !== undefined && typeof row.errorMessage !== 'string') {
        malformed(`invalid errorMessage for ${clientOrderId}`);
      }
      if (row.success && (typeof row.orderId !== 'string' || !/^[1-9]\d*$/.test(row.orderId))) {
        malformed(`invalid orderId for ${clientOrderId}`);
      }

      resultsByClientOrderId.set(clientOrderId, row);
    }

    const orderedResults = clientOrderIds.map((clientOrderId) => {
      const result = resultsByClientOrderId.get(clientOrderId);
      return result || malformed(`missing clientOrderId ${clientOrderId}`);
    });

    orderedResults.forEach((result, index) => {
      if (result.success && orders[index].timeInForce === OrderTimeInForce.PostOnly) {
        this.store.updateOrder({ id: result.orderId as string }, { timeInForce: OrderTimeInForce.PostOnly });
      }
    });

    const rejectedResults = orderedResults.filter((result) => !result.success);
    if (rejectedResults.length) {
      const message = rejectedResults
        .map((result) => `${result.clientOrderId}: ${result.errorMessage || result.errorCode || 'WEEX order rejected'}`)
        .join('; ');
      this.emitAndLogError(message);
      throw new Error(message);
    }

    return orderedResults.map((result) => result.orderId as string);
  };

  private isAlgoOrder = (order: Order) =>
    order.type === OrderType.Conditional ||
    order.type === OrderType.StopLoss ||
    order.type === OrderType.TakeProfit ||
    order.type === OrderType.TrailingStopLoss ||
    order.id.includes('__');

  private inferPositionSide = (opts: Pick<PlaceOrderOpts, 'side' | 'reduceOnly'>): PositionSide => {
    if (opts.reduceOnly) {
      return opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    }
    return opts.side === OrderSide.Buy ? PositionSide.Long : PositionSide.Short;
  };

  private formatOrderPayload = (opts: PlaceOrderOpts): Record<string, any> => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      const message = `Market ${opts.symbol} not found`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const amount = adjust(opts.amount, market.precision.amount);
    const price = opts.price ? adjust(opts.price, market.precision.price) : undefined;
    const positionSide = REVERSE_POSITION_SIDE[this.inferPositionSide(opts)];

    if (opts.type === OrderType.Conditional) {
      const triggerPrice = opts.triggerPrice;
      if (typeof triggerPrice !== 'number' || !Number.isFinite(triggerPrice) || triggerPrice <= 0) {
        const message = 'Trigger price is required for WEEX opening conditional orders';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      return omitUndefined({
        symbol: opts.symbol,
        side: REVERSE_ORDER_SIDE[opts.side],
        positionSide: REVERSE_POSITION_SIDE[this.inferPositionSide({ side: opts.side, reduceOnly: false })],
        type: opts.price ? 'STOP' : 'STOP_MARKET',
        quantity: `${amount}`,
        price: opts.price ? `${price}` : undefined,
        triggerPrice: `${adjust(triggerPrice, market.precision.price)}`,
        clientAlgoId: `ts${uuid()}`.slice(0, 36),
        presetTakeProfitPrice: opts.takeProfit ? `${adjust(opts.takeProfit, market.precision.price)}` : undefined,
        presetStopLossPrice: opts.stopLoss ? `${adjust(opts.stopLoss, market.precision.price)}` : undefined,
        TpWorkingType: opts.takeProfit
          ? REVERSE_ORDER_TRIGGER_TYPE[opts.tpTriggerType || OrderStopTriggerType.LastPrice]
          : undefined,
        SlWorkingType: opts.stopLoss
          ? REVERSE_ORDER_TRIGGER_TYPE[opts.slTriggerType || OrderStopTriggerType.MarkPrice]
          : undefined,
      });
    }

    if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) {
      if (!opts.price) {
        const message = 'Trigger price is required for WEEX TP/SL orders';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      return omitUndefined({
        symbol: opts.symbol,
        clientAlgoId: `ts${uuid()}`.slice(0, 36),
        planType: opts.type === OrderType.TakeProfit ? 'TAKE_PROFIT' : 'STOP_LOSS',
        triggerPrice: `${price}`,
        executePrice: '0',
        quantity: `${amount}`,
        positionSide,
        triggerPriceType:
          REVERSE_ORDER_TRIGGER_TYPE[
            opts.type === OrderType.TakeProfit
              ? opts.tpTriggerType || OrderStopTriggerType.LastPrice
              : opts.slTriggerType || OrderStopTriggerType.MarkPrice
          ],
      });
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      const message = 'WEEX trailing stop loss is not supported';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    return omitUndefined({
      symbol: opts.symbol,
      side: REVERSE_ORDER_SIDE[opts.side],
      positionSide,
      type: REVERSE_ORDER_TYPE[opts.type],
      timeInForce:
        opts.type === OrderType.Limit
          ? REVERSE_TIME_IN_FORCE[opts.timeInForce || OrderTimeInForce.GoodTillCancel]
          : undefined,
      quantity: `${amount}`,
      price: opts.type === OrderType.Limit ? `${price}` : undefined,
      newClientOrderId: this.resolveCallerClientOrderId(opts) ?? `ts${uuid()}`.slice(0, 36),
      reduceOnly: opts.reduceOnly || undefined,
      tpTriggerPrice: opts.takeProfit ? `${adjust(opts.takeProfit, market.precision.price)}` : undefined,
      slTriggerPrice: opts.stopLoss ? `${adjust(opts.stopLoss, market.precision.price)}` : undefined,
      TpWorkingType: opts.takeProfit
        ? REVERSE_ORDER_TRIGGER_TYPE[opts.tpTriggerType || OrderStopTriggerType.LastPrice]
        : undefined,
      SlWorkingType: opts.stopLoss
        ? REVERSE_ORDER_TRIGGER_TYPE[opts.slTriggerType || OrderStopTriggerType.MarkPrice]
        : undefined,
    });
  };
}
