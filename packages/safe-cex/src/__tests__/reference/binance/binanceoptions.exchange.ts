import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';
import type {
  AccountGreeks,
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalOrder,
  Market,
  OHLCVOptions,
  OptionChainQuery,
  OptionChainRow,
  Order,
  OrderHistoryOptions,
  PlaceOrderOpts,
  Position,
  Ticker,
  UpdateOrderOpts,
} from '../../../types';
import type { BinanceOptionContract } from './binanceoptions.mappers';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import { mapSeries } from 'p-iteration';

import { LogSeverity, OrderStatus, OrderType } from '../../../types';
import { loop } from '../../../utils/loop';
import { adjust } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { buildOptionChain, optionExpiries } from '../bybit/bybit.options';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './binanceoptions.api';
import {
  formatBinanceOptionOrder,
  mapBinanceAccountGreeks,
  mapBinanceOptionBalance,
  mapBinanceOptionCandle,
  mapBinanceOptionFill,
  mapBinanceOptionHistoricalOrder,
  mapBinanceOptionMarket,
  mapBinanceOptionOrder,
  mapBinanceOptionPosition,
  mapBinanceOptionQuote,
  mapBinanceOptionTicker,
  optionContractsByUnderlying,
} from './binanceoptions.mappers';
import { ENDPOINTS, GRAFANA_ENDPOINTS_EVERY, GRAFANA_ENDPOINTS_PERIODIC } from './binanceoptions.types';

type Row = Record<string, any>;

/**
 * Binance European options (eapi.binance.com): a separate options account, the same API key, and the
 * same query-string HMAC as the rest of the Binance family. REST only; see CLAUDE.md for the gaps.
 */
export class BinanceOptionsExchange extends BaseExchange {
  // Binance documents no charset or length for option client IDs; this is the subset every Binance product takes.
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Binance Options', /^[A-Za-z0-9_-]{1,36}$/);

  xhr: Axios;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private contracts = new Map<string, BinanceOptionContract>();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);
    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
  }

  validateAccount = async () => {
    try {
      return { error: '', balance: await this.fetchBalance() };
    } catch (err: any) {
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
    const markets = await this.fetchMarkets();
    if (this.isDisposed) return;
    this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });

    const tickers = await this.fetchTickers();
    if (this.isDisposed) return;
    this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
    this.log(`Loaded ${markets.length} Binance option markets`);

    if (this.isPublicDataOnly) {
      this.store.update({
        orders: [],
        positions: [],
        balance: { used: 0, free: 0, total: 0, upnl: 0 },
        loaded: { ...this.store.loaded, orders: true, balance: true, positions: true },
      });
      return;
    }

    await this.tick();
    if (this.isDisposed) return;
    const orders = await this.fetchOrders();
    if (this.isDisposed) return;
    this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
    this.log('Ready to trade on Binance Options');
  };

  /** No websockets yet, so the tick polls everything the store shows. */
  tick = async () => {
    if (this.isDisposed) return;
    try {
      const [balance, positions, tickers] = await Promise.all([
        this.fetchBalance(),
        this.fetchPositions(),
        this.fetchTickers(),
      ]);
      if (this.isDisposed) return;
      this.store.update({
        balance,
        positions,
        tickers,
        loaded: { ...this.store.loaded, balance: true, positions: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err?.message);
    }
    if (!this.isDisposed) loop(() => this.tick(), this.getTickInterval());
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const { data } = await this.xhr.get<Row>(ENDPOINTS.MARKETS);
      this.contracts = optionContractsByUnderlying(data);
      return ((data.optionSymbols ?? []) as Row[]).flatMap((raw) => {
        const market = mapBinanceOptionMarket(raw, this.contracts);
        return market ? [market] : [];
      });
    } catch (err: any) {
      if (!this.store.markets.length) throw err;
      this.log(`Failed to load Binance option markets, keeping the last ones: ${err?.message}`, LogSeverity.Warning);
      return this.store.markets;
    }
  };

  private fetchTickerAndMarkRows = async (): Promise<{ tickers: Row[]; marks: Row[] }> => {
    const [{ data: tickers }, { data: marks }] = await Promise.all([
      this.xhr.get<Row[]>(ENDPOINTS.TICKERS),
      this.xhr.get<Row[]>(ENDPOINTS.MARK),
    ]);
    return { tickers, marks };
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    const { tickers, marks } = await this.fetchTickerAndMarkRows();
    const markBySymbol = new Map(marks.map((m) => [m.symbol, m]));
    return tickers
      .filter((t) => this.store.marketsBySymbol[t.symbol])
      .map((t) => mapBinanceOptionTicker(t, markBySymbol.get(t.symbol)));
  };

  fetchOptionChain = async (query: OptionChainQuery): Promise<OptionChainRow[]> => {
    const { tickers, marks } = await this.fetchTickerAndMarkRows();
    const tickerBySymbol = new Map(tickers.map((t) => [t.symbol, t]));
    const quotes = marks.map((m) => mapBinanceOptionQuote(m, tickerBySymbol.get(m.symbol)));
    return buildOptionChain(this.store.markets, quotes, query);
  };

  fetchOptionExpiries = async (underlying: string): Promise<number[]> => optionExpiries(this.store.markets, underlying);

  fetchAccountGreeks = async (): Promise<AccountGreeks[]> => {
    const { data } = await this.xhr.get<Row>(ENDPOINTS.ACCOUNT);
    return mapBinanceAccountGreeks(data, this.contracts);
  };

  fetchBalance = async (): Promise<Balance> => {
    const { data } = await this.xhr.get<Row>(ENDPOINTS.ACCOUNT);
    return mapBinanceOptionBalance(data);
  };

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    const only = symbols?.length === 1 ? { symbol: symbols[0] } : {};
    const [{ data: rows }, { data: marks }] = await Promise.all([
      this.xhr.get<Row[]>(ENDPOINTS.POSITIONS, { params: only }),
      this.xhr.get<Row[]>(ENDPOINTS.MARK, { params: only }),
    ]);
    const markBySymbol = new Map(marks.map((m) => [m.symbol, m]));
    const now = virtualClock.getCurrentTimeMs();
    return rows
      .filter((p) => parseFloat(p.quantity) !== 0 && (!symbols?.length || symbols.includes(p.symbol)))
      .map((p) => mapBinanceOptionPosition(p, this.store.marketsBySymbol[p.symbol], markBySymbol.get(p.symbol), now));
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    const params = symbols?.length === 1 ? { symbol: symbols[0] } : {};
    const { data } = await this.xhr.get<Row[]>(ENDPOINTS.OPEN_ORDERS, { params });
    const now = virtualClock.getCurrentTimeMs();
    return data.filter((o) => !symbols?.length || symbols.includes(o.symbol)).map((o) => mapBinanceOptionOrder(o, now));
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    const limit = Math.min(opts.limit || 500, 1000);
    const [, amount, unit] = opts.interval.split(/(\d+)/);
    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);
    const { data } = await this.xhr.get<unknown[][]>(ENDPOINTS.KLINE, {
      params: {
        symbol: opts.symbol,
        interval: opts.interval,
        startTime: start.valueOf(),
        endTime: end.valueOf(),
        ...(opts.limit ? { limit } : {}),
      },
    });
    return data.map(mapBinanceOptionCandle);
  };

  fetchHistoricalOrders = async (opts: OrderHistoryOptions = {}): Promise<HistoricalOrder[]> => {
    if (!opts.symbol) throw new Error('Binance options order history needs a symbol');
    const { data } = await this.xhr.get<Row[]>(ENDPOINTS.HISTORY_ORDERS, {
      params: { symbol: opts.symbol, orderId: opts.orderId, startTime: opts.from, endTime: opts.to, limit: opts.limit },
    });
    const orders = data.map(mapBinanceOptionHistoricalOrder);
    return opts.status?.length ? orders.filter((o) => opts.status!.includes(o.status)) : orders;
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (!opts.symbol) throw new Error('Binance options fills need a symbol');
    const { data } = await this.xhr.get<Row[]>(ENDPOINTS.USER_TRADES, {
      params: { symbol: opts.symbol, startTime: opts.from, endTime: opts.to, limit: opts.limit },
    });
    const fills = data.map(mapBinanceOptionFill);
    return opts.orderId ? fills.filter((f) => f.orderId === opts.orderId) : fills;
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    const clientOrderId = this.resolveCallerClientOrderId(opts);
    this.assertOptionOrderPlaceable(opts);
    return [await this.sendOrder(opts, clientOrderId)];
  };

  /** One request per order: a batch is all-or-nothing on preflight, then placed in sequence. */
  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    for (const order of orders) this.assertOptionOrderPlaceable(order);
    return await mapSeries(orders, (order) => this.sendOrder(order, order.clientOrderId));
  };

  private assertOptionOrderPlaceable = (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) this.rejectPreflight(`Market ${opts.symbol} not found`);
    if (opts.impliedVolatility !== undefined) {
      this.rejectPreflight(`Binance options take no implied volatility on an order; give a price (${opts.symbol})`);
    }
    if (opts.type !== OrderType.Limit) {
      this.rejectPreflight(`Binance options take limit orders only, not ${opts.type} (${opts.symbol})`);
    }
    for (const field of ['triggerPrice', 'stopLoss', 'takeProfit'] as const) {
      if (opts[field] !== undefined) this.rejectPreflight(`Binance options do not support ${field} (${opts.symbol})`);
    }
    if (!opts.price) this.rejectPreflight(`A Binance option order needs a price (${opts.symbol})`);
    const amount = adjust(opts.amount, market.precision.amount);
    if (!(amount > 0)) this.rejectPreflight(`${opts.amount} rounds to nothing on ${opts.symbol}`);
    if (market.limits.amount.max > 0 && amount > market.limits.amount.max) {
      this.rejectPreflight(
        `${amount} is above the ${market.limits.amount.max} maximum for one order on ${opts.symbol}`,
      );
    }
  };

  private sendOrder = async (opts: PlaceOrderOpts, clientOrderId: string | undefined): Promise<string> => {
    const market = this.store.marketsBySymbol[opts.symbol]!;
    const payload = formatBinanceOptionOrder(opts, market, clientOrderId ?? uuid());
    const { data } = await this.xhr.post<Row>(ENDPOINTS.ORDER, payload);
    return String(data.orderId);
  };

  cancelOrders = async (orders: Order[]) => {
    for (const order of orders) {
      try {
        await this.xhr.delete(ENDPOINTS.ORDER, { params: { symbol: order.symbol, orderId: order.id } });
      } catch (err: any) {
        // -2011: Binance no longer has it open, which is the state a cancel asks for.
        if (err?.code !== -2011 && err?.response?.data?.code !== -2011) throw err;
      }
      this.store.updateOrdersStatus([order], OrderStatus.Canceled);
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    await this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, { params: { symbol } });
    this.store.updateOrdersStatus(
      this.store.orders.filter((o) => o.symbol === symbol),
      OrderStatus.Canceled,
    );
  };

  /**
   * Binance has no option amend: check the replacement, cancel, then place it with a new
   * generated client ID. The check runs first so a refused amend leaves the order open.
   */
  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const replacement: PlaceOrderOpts = {
      symbol: order.symbol,
      type: OrderType.Limit,
      side: order.side,
      price: 'price' in update ? update.price : order.price,
      amount: 'amount' in update ? update.amount : order.remaining || order.amount,
      reduceOnly: order.reduceOnly,
      timeInForce: order.timeInForce,
    };
    this.assertOptionOrderPlaceable(replacement);
    await this.cancelOrders([order]);
    return [await this.sendOrder(replacement, undefined)];
  };
}
