import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';

import {
  Balance,
  Candle,
  ExchangeAccount,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
  ValidateAccountResult,
} from '../../../types';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createKucoinSpotAPI } from './kucoinspot.api';
import {
  KUCOIN_SPOT_ENDPOINTS,
  KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY,
  KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC,
  KUCOIN_SPOT_INTERVAL,
  KUCOIN_SPOT_ORDER_STATUS,
  KUCOIN_SPOT_ORDER_TYPE,
  KUCOIN_SPOT_TIME_IN_FORCE,
} from './kucoinspot.types';
import { KucoinSpotPrivateWebsocket } from './kucoinspot.ws-private';
import { KucoinSpotPublicWebsocket } from './kucoinspot.ws-public';

// KuCoin Spot's open-orders-by-page endpoint rejects pageSize > 50
// ("pageSize must be less or equals than 50"), which made fetchOrders error out
// entirely — open orders never loaded. Keep this at the 50 max.
const KUCOIN_SPOT_OPEN_ORDERS_PAGE_SIZE = 50;
const KUCOIN_SPOT_OPEN_ORDERS_MAX_PAGES = 100;

export class KucoinSpotExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('KuCoin Spot', /^[A-Za-z0-9_-]{1,40}$/);

  xhr: Axios;
  unlimitedXHR: Axios;

  isSpot = true;

  publicWebsocket: KucoinSpotPublicWebsocket;
  privateWebsocket: KucoinSpotPrivateWebsocket;

  grafanaEndpointsEvery = KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createKucoinSpotAPI, {
      rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }),
    });
    this.unlimitedXHR = createKucoinSpotAPI(opts, this);

    this.publicWebsocket = new KucoinSpotPublicWebsocket(this);
    this.privateWebsocket = new KucoinSpotPrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  getAccount = async (): Promise<ExchangeAccount> => {
    try {
      const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.ACCOUNT_SUMMARY);

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to fetch KuCoin Spot account summary');
        return { userId: '', subId: null, affiliateId: null };
      }

      return {
        userId: data.data?.userId || data.data?.uid || '',
        subId: data.data?.subUserId || null,
        affiliateId: data.data?.referrerUserId || null,
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { userId: '', subId: null, affiliateId: null };
    }
  };

  validateAccount = async (): Promise<ValidateAccountResult> => {
    try {
      const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.ACCOUNTS, {
        params: { type: 'trade' },
      });

      if (data.code !== '200000') {
        return { error: data.msg || 'Failed to validate KuCoin Spot account' };
      }

      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      const errorMessage = err?.response?.data?.msg || err?.message || 'Failed to validate KuCoin Spot account';
      this.emitAndLogError(errorMessage);
      return { error: errorMessage };
    }
  };

  start = async (): Promise<void> => {
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
      this.log(`Loaded ${Math.min(markets.length, tickers.length)} KuCoin Spot markets`);
    }

    const positions = await this.fetchPositions();
    if (this.isDisposed) return;

    this.store.addOrUpdatePositionsFull(positions);

    if (this.isPublicDataOnly) {
      this.store.update({
        orders: [],
        balance: {
          used: 0,
          free: 0,
          total: 0,
          upnl: 0,
        },
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      this.log(`Ready to watch public data on KuCoin Spot`);
      return;
    }

    this.privateWebsocket.connectAndSubscribe();
    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on KuCoin Spot`);
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.ACCOUNTS, {
        params: { type: 'trade' },
      });

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to fetch KuCoin Spot balance');
        return this.store.balance;
      }

      const currencies: NonNullable<Balance['currencies']> = {};
      const coins: NonNullable<Balance['coins']> = {};
      let free = 0;
      let used = 0;
      let total = 0;

      for (const account of data.data || []) {
        const currency = account.currency;
        const available = parseFloat(account.available || '0');
        const holds = parseFloat(account.holds || '0');
        const balance = parseFloat(account.balance || '0');
        const price = this.getUsdPrice(currency);

        currencies[currency] = {
          free: available,
          used: holds,
          total: balance,
          upnl: 0,
          info: account,
        };

        coins[currency] = {
          base: currency,
          amount: balance,
          notional: balance * price,
        };

        free += available * price;
        used += holds * price;
        total += balance * price;
      }

      return {
        free,
        used,
        total,
        upnl: 0,
        currencies,
        coins,
        info: data.data,
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.balance;
    }
  };

  private getUsdPrice = (currency: string): number => {
    if (currency === 'USD' || currency === 'USDT' || currency === 'USDC') return 1;

    const ticker =
      this.store.tickersBySymbol[`${currency}-USDT`] ||
      this.store.tickersBySymbol[`${currency}-USDC`] ||
      this.store.tickersBySymbol[`${currency}-USD`];

    return ticker?.last || 0;
  };

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    const symbolSet = symbols ? new Set(symbols) : null;

    return this.store.markets
      .filter((market) => market.active && (!symbolSet || symbolSet.has(market.symbol)))
      .map((market) => ({
        id: `${market.symbol}:long`,
        symbol: market.symbol,
        side: PositionSide.Long,
        entryPrice: 0,
        notional: 0,
        leverage: 1,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        marginType: MarginType.Isolated,
        positionMode: PositionMode.OneWay,
        info: { spot: true },
      }));
  };

  fetchSymbolMaxLeverage = async (_symbol: string): Promise<number> => {
    return 1;
  };

  setLeverage = async (symbol: string, _leverage: number, _side?: PositionSide): Promise<void> => {
    this.store.update({
      fetchedLeverage: {
        ...this.store.fetchedLeverage,
        [symbol]: { buyLeverage: 1, sellLeverage: 1 },
      },
    });
  };

  changeMarginType = async (_isolated: boolean, _symbol?: string): Promise<void> => {};

  changePositionMode = async (_hedged: boolean, _symbol?: string): Promise<void> => {};

  tick = async (): Promise<void> => {
    if (this.isDisposed || this.isPublicDataOnly) return;

    try {
      const [orders, positions, balance] = await Promise.all([
        this.fetchOrders(),
        this.fetchPositions(),
        this.fetchBalance(),
      ]);
      if (this.isDisposed) return;

      this.store.addOrUpdatePositionsFull(positions);
      this.store.update({
        orders,
        balance,
        loaded: {
          ...this.store.loaded,
          balance: true,
          positions: true,
          orders: true,
        },
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }

    if (!this.isDisposed) {
      loop(() => this.tick(), this.getTickInterval());
    }
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    try {
      const fetchSymbolOrders = async (symbol?: string): Promise<Order[]> => {
        const orders: Order[] = [];
        let pageNum = 1;
        let totalPage = 1;

        while (pageNum <= totalPage && pageNum <= KUCOIN_SPOT_OPEN_ORDERS_MAX_PAGES) {
          const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.OPEN_ORDERS, {
            params: omitUndefined({
              symbol,
              pageNum,
              pageSize: KUCOIN_SPOT_OPEN_ORDERS_PAGE_SIZE,
            }),
          });

          if (data.code !== '200000') {
            this.emitAndLogError(data.msg || 'Failed to fetch KuCoin Spot orders');
            return orders;
          }

          const page = data.data || {};
          const items = Array.isArray(page.items) ? page.items : [];

          orders.push(...items.map((order: Record<string, any>) => this.mapOrder(order)));

          const nextTotalPage = parseInt(`${page.totalPage ?? ''}`, 10);
          if (Number.isFinite(nextTotalPage)) {
            totalPage = nextTotalPage;
          }

          const hasMorePages = Number.isFinite(nextTotalPage)
            ? pageNum < totalPage
            : items.length >= KUCOIN_SPOT_OPEN_ORDERS_PAGE_SIZE;
          if (!hasMorePages) {
            break;
          }

          pageNum += 1;
        }

        return orders;
      };

      const targetSymbols = symbols?.length ? symbols : await this.fetchActiveOrderSymbols();
      if (!targetSymbols.length) return [];
      const ordersBySymbol = await Promise.all(targetSymbols.map((symbol) => fetchSymbolOrders(symbol)));
      return ordersBySymbol.flat();
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.orders;
    }
  };

  // The HF open-orders endpoint REQUIRES a symbol ("Symbol can't be empty"), so
  // an all-symbols fetch (the tick loop + WS refresh-heal both call fetchOrders()
  // with no args) must first discover which symbols currently have active orders.
  private fetchActiveOrderSymbols = async (): Promise<string[]> => {
    try {
      const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.ACTIVE_ORDER_SYMBOLS);
      if (data.code !== '200000') return [];
      return Array.isArray(data.data?.symbols) ? data.data.symbols : [];
    } catch {
      return [];
    }
  };

  mapOrder = (order: Record<string, any>): Order => {
    const size = parseFloat(order.size || order.originSize || '0');
    const filled = parseFloat(order.dealSize || order.filledSize || '0');
    const remaining = parseFloat(order.remainSize ?? `${Math.max(0, size - filled)}`);
    const timeInForce =
      order.postOnly === true
        ? OrderTimeInForce.PostOnly
        : KUCOIN_SPOT_TIME_IN_FORCE[order.timeInForce] || OrderTimeInForce.GoodTillCancel;

    return {
      id: order.id || order.orderId,
      status: KUCOIN_SPOT_ORDER_STATUS[order.status] || KUCOIN_SPOT_ORDER_STATUS[order.active ? 'active' : 'done'],
      symbol: order.symbol,
      type: KUCOIN_SPOT_ORDER_TYPE[order.type || order.orderType] || KUCOIN_SPOT_ORDER_TYPE.limit,
      side: order.side === 'sell' ? OrderSide.Sell : OrderSide.Buy,
      price: parseFloat(order.price || order.matchPrice || '0'),
      amount: size,
      filled,
      remaining,
      reduceOnly: false,
      timeInForce,
      lastUpdate: parseInt(order.lastUpdatedAt || order.orderTime || order.createdAt || `${Date.now()}`, 10),
      info: order,
    };
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    const clientOrderId = this.resolveCallerClientOrderId(opts);
    if (opts.type !== OrderType.Limit && opts.type !== OrderType.Market) {
      const msg = `KuCoin Spot does not support ${opts.type} orders`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    if (opts.stopLoss || opts.takeProfit) {
      const msg = 'KuCoin Spot attached TP/SL orders are not supported';
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    const amount = adjust(opts.amount, market.precision.amount);
    const price = opts.price && opts.type === OrderType.Limit ? adjust(opts.price, market.precision.price) : undefined;

    const timeInForce =
      opts.timeInForce === OrderTimeInForce.ImmediateOrCancel
        ? 'IOC'
        : opts.timeInForce === OrderTimeInForce.FillOrKill
          ? 'FOK'
          : 'GTC';

    const payload = omitUndefined({
      clientOid: clientOrderId ?? uuid(),
      symbol: market.id,
      type: opts.type === OrderType.Market ? 'market' : 'limit',
      side: opts.side === OrderSide.Sell ? 'sell' : 'buy',
      price: price !== undefined ? `${price}` : undefined,
      size: `${amount}`,
      timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
      postOnly: opts.timeInForce === OrderTimeInForce.PostOnly ? true : undefined,
    });

    try {
      const { data } = await this.unlimitedXHR.post(KUCOIN_SPOT_ENDPOINTS.PLACE_ORDER, payload);

      if (data.code !== '200000') {
        throw new Error(data.msg || 'Failed to place KuCoin Spot order');
      }

      return data.data?.orderId ? [data.data.orderId] : [];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts): Promise<string[]> => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    const payload = omitUndefined({
      symbol: market.id,
      orderId: order.id,
      newPrice:
        'price' in update && update.price !== undefined ? `${adjust(update.price, market.precision.price)}` : undefined,
      newSize:
        'amount' in update && update.amount !== undefined
          ? `${adjust(update.amount, market.precision.amount)}`
          : undefined,
    });

    try {
      const { data } = await this.unlimitedXHR.post(KUCOIN_SPOT_ENDPOINTS.MODIFY_ORDER, payload);

      if (data.code !== '200000') {
        throw new Error(data.msg || `Failed to update KuCoin Spot order ${order.id}`);
      }

      return data.data?.newOrderId ? [data.data.newOrderId] : [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    for (const order of orders) {
      const market = this.store.marketsBySymbol[order.symbol];
      const symbol = market?.id || order.symbol;

      try {
        const { data } = await this.unlimitedXHR.delete(`${KUCOIN_SPOT_ENDPOINTS.CANCEL_ORDER}/${order.id}`, {
          params: { symbol },
        });

        if (data.code !== '200000') {
          throw new Error(data.msg || `Failed to cancel KuCoin Spot order ${order.id}`);
        }
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    const market = this.store.marketsBySymbol[symbol];
    const apiSymbol = market?.id || symbol;

    try {
      const { data } = await this.unlimitedXHR.delete(KUCOIN_SPOT_ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
        params: { symbol: apiSymbol },
      });

      if (data.code !== '200000') {
        throw new Error(data.msg || `Failed to cancel KuCoin Spot orders for ${symbol}`);
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelAllOrders = async (): Promise<void> => {
    try {
      const { data } = await this.unlimitedXHR.delete(KUCOIN_SPOT_ENDPOINTS.CANCEL_ALL_ORDERS);

      if (data.code !== '200000') {
        throw new Error(data.msg || 'Failed to cancel all KuCoin Spot orders');
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    const { symbol, orderId, from, to, limit } = opts;
    if (!symbol) {
      return [];
    }

    try {
      const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.FILLS, {
        params: omitUndefined({
          symbol,
          orderId,
          limit: limit ? Math.min(limit, 100) : 100,
          startAt: from,
          endAt: to,
        }),
      });

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to fetch KuCoin Spot executions');
        return [];
      }

      return (data.data?.items || []).map(
        (fill: Record<string, any>): Execution => ({
          timestamp: parseInt(fill.createdAt || `${Date.now()}`, 10),
          symbol: fill.symbol,
          orderId: fill.orderId,
          price: parseFloat(fill.price || '0'),
          qty: parseFloat(fill.size || '0'),
          side: fill.side === 'sell' ? OrderSide.Sell : OrderSide.Buy,
          commission: parseFloat(fill.fee || '0'),
          commissionAsset: fill.feeCurrency,
        }),
      );
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.MARKETS);

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to fetch KuCoin Spot markets');
        return this.store.markets;
      }

      return data.data.map((m: Record<string, any>) => ({
        id: m.symbol,
        symbol: m.symbol,
        prettySymbol: `${m.baseCurrency}${m.quoteCurrency}`,
        base: m.baseCurrency,
        quote: m.quoteCurrency,
        active: m.enableTrading === true,
        precision: {
          amount: parseFloat(m.baseIncrement || '1'),
          price: parseFloat(m.priceIncrement || '1'),
        },
        limits: {
          amount: {
            min: parseFloat(m.baseMinSize || '0'),
            max: parseFloat(m.baseMaxSize || `${Number.MAX_SAFE_INTEGER}`),
          },
          leverage: {
            min: 1,
            max: 1,
          },
        },
        contractSize: 1,
        isSpot: true,
        isInverse: false,
        info: m,
      }));
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.TICKERS);

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to fetch KuCoin Spot tickers');
        return this.store.tickers;
      }

      return (data.data?.ticker || []).reduce((tickers: Ticker[], t: Record<string, any>) => {
        const market = this.store.marketsBySymbol[t.symbol];
        if (!market) return tickers;

        const last = parseFloat(t.last || '0');

        tickers.push({
          id: market.id,
          symbol: market.symbol,
          bid: parseFloat(t.buy || '0'),
          ask: parseFloat(t.sell || '0'),
          last,
          mark: last,
          index: last,
          percentage: parseFloat(t.changeRate || '0') * 100,
          fundingRate: 0,
          fundingTime: 0,
          openInterest: 0,
          volume: parseFloat(t.vol || '0'),
          quoteVolume: parseFloat(t.volValue || '0'),
          info: t,
        });

        return tickers;
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    const limit = Math.min(opts.limit || 500, 1500);
    const interval = KUCOIN_SPOT_INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.xhr.get(KUCOIN_SPOT_ENDPOINTS.KLINE, {
      params: {
        symbol: opts.symbol,
        type: interval,
        startAt: Math.floor(start.valueOf() / 1000),
        endAt: Math.floor(end.valueOf() / 1000),
      },
    });

    if (data.code !== '200000') {
      this.emitAndLogError(data.msg || 'Failed to fetch KuCoin Spot candles');
      return [];
    }

    return data.data
      .map(
        (c: any[]): Candle => ({
          timestamp: parseInt(c[0], 10),
          open: parseFloat(c[1]),
          close: parseFloat(c[2]),
          high: parseFloat(c[3]),
          low: parseFloat(c[4]),
          volume: parseFloat(c[5]),
        }),
      )
      .sort((a: Candle, b: Candle) => a.timestamp - b.timestamp);
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
}
