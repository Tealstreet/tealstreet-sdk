import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import groupBy from 'lodash/groupBy';
import omit from 'lodash/omit';
import times from 'lodash/times';
import { forEachSeries, mapSeries } from 'p-iteration';

import {
  Balance,
  Candle,
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
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { add, adjust, subtract } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import type { ClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI, FETCH_POSITIONS_RESPONSE } from './bingx.api';
import {
  BINGX_BALANCE,
  ENDPOINTS,
  FETCH_BALANCE_RESPONSE,
  FETCH_HEDGE_MODE_RESPONSE,
  FETCH_LEVERAGE_RESPONSE,
  FETCH_MARGIN_MODE_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from './bingx.types';
import { BingxPrivateWebsocket } from './bingx.ws-private';
import { BingxPublicWebsocket } from './bingx.ws-public';

export class BingxExchange extends BaseExchange {
  // BingX lowercases client order IDs server-side, so an uppercase ID could never be queried back.
  protected readonly clientOrderIdRule: ClientOrderIdRule = {
    label: 'BingX',
    mint: () => `ts${uuid()}`.toLowerCase().slice(0, 40),
    requirement: 'be 1-40 characters with no uppercase letters',
    isValid: (clientOrderId) =>
      clientOrderId.length >= 1 && clientOrderId.length <= 40 && clientOrderId === clientOrderId.toLowerCase(),
  };

  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BingxPublicWebsocket;
  privateWebsocket: BingxPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private _accountPositionMode: PositionMode | null = null;

  toBingxSymbol = (symbol: string) => {
    const market = this.store.marketsBySymbol[symbol] || this.store.marketsById[symbol];
    if (market?.id) return market.id;
    if (symbol.includes('-')) return symbol;

    const quoteSuffixMatch = symbol.match(/(USDT|USDC)$/);
    if (quoteSuffixMatch) {
      const quote = quoteSuffixMatch[1];
      return `${symbol.slice(0, -quote.length)}-${quote}`;
    }

    return symbol;
  };

  private isLikelyTradfiMarket = (market: Market) => {
    const contractSymbol = (market.id || '').split('-')[0] || market.base || '';
    return /USD$/i.test(contractSymbol) && !/(USDT|USDC)$/i.test(contractSymbol);
  };

  private buildTradfiOneWayModeError = (symbol: string, originalErrorMessage?: string) => {
    const details = originalErrorMessage ? ` (BingX message: ${originalErrorMessage})` : '';
    return `BingX OpenAPI requires Hedged position mode for non-crypto symbols. Switch this BingX account to Hedged mode and retry (${symbol}).${details}`;
  };

  private createClientOrderId = () => `ts${uuid()}`.toLowerCase().slice(0, 40);

  private normalizeOrderPlacementError = (symbol: string, errorMessage: string) => {
    if (/non-crypto symbol with one-way mode not support openapi and copy trade/i.test(errorMessage)) {
      return this.buildTradfiOneWayModeError(symbol, errorMessage);
    }

    return errorMessage;
  };

  deriveTickerBidAsk = (market: Pick<Market, 'precision'>, last: number) => {
    if (!Number.isFinite(last) || last <= 0) {
      return { bid: 0, ask: 0 };
    }
    const priceStep = Number.isFinite(market.precision?.price) && market.precision.price > 0 ? market.precision.price : 0;
    return {
      bid: Math.max(subtract(last, priceStep), 0),
      ask: add(last, priceStep),
    };
  };

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BingxPublicWebsocket(this);
    this.privateWebsocket = new BingxPrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  private fetchPrimaryBalanceData = async (): Promise<BINGX_BALANCE> => {
    const { data } = await this.xhr.get<FETCH_BALANCE_RESPONSE>(ENDPOINTS.BALANCE);
    if (data.code !== 0) {
      throw new Error(data.msg || `BingX balance request failed with code ${data.code}`);
    }

    const balance = data.data?.find((row) => row.asset === 'USDT') ?? data.data?.[0];
    if (!balance) {
      throw new Error('BingX balance response did not include an account balance');
    }

    return balance;
  };

  getAccount = async () => {
    const balance = await this.fetchPrimaryBalanceData();
    return { userId: String(balance.userId) };
  };

  validateAccount = async () => {
    let balance: Balance | undefined;
    try {
      const bal = await this.fetchPrimaryBalanceData();
      const total = parseFloat(bal?.balance);
      if (Number.isFinite(total)) {
        balance = {
          total,
          free: parseFloat(bal?.availableMargin) || 0,
          used: parseFloat(bal?.usedMargin) || 0,
          upnl: parseFloat(bal?.unrealizedProfit) || 0,
        };
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
    return { error: '', balance };
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    if (!useExternalPublicData) {
      const [markets, rawTickers] = await Promise.all([
        this.fetchMarkets().then((markets) => {
          if (this.isDisposed) return [];
          this.store.update({
            markets,
            loaded: { ...this.store.loaded, markets: true },
          });

          // start public websocket streams as soon as markets loaded.
          this.publicWebsocket.connectAndSubscribe();

          return markets;
        }),
        this.fetchTickersRaw(),
      ]);

      if (this.isDisposed) return;

      const tickers = this.formatTickers(rawTickers);
      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Bingx markets`);
    }

    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    // fetch current position mode (Hedge/One-way)
    await this.getOrFetchPositionMode(true);

    await Promise.all([
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
      this.fetchBalance().then((balance) => {
        if (this.isDisposed) return;
        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });
        return balance;
      }),
    ]);

    if (this.isDisposed) return;

    await this.tick();

    this.log(`Ready to trade on Bingx`);
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        let orders: Order[] | undefined;
        if ((count + 2) % 4 === 0) {
          orders = await this.fetchOrders();
          if (this.isDisposed) return;
          this.store.update({
            orders,
            loaded: {
              ...this.store.loaded,
              orders: true,
            },
          });
        }
        let balance: Balance | undefined;
        if (count % 4 === 0) {
          balance = await this.fetchBalance();
          if (this.isDisposed) return;
          this.store.update({
            balance,
            loaded: {
              ...this.store.loaded,
              balance: true,
            },
          });
        }

        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.store.addOrUpdatePositionsFull(positions);
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), this.getTickInterval());
      }
    }
  };

  fetchBalance = async () => {
    try {
      const data = await this.fetchPrimaryBalanceData();

      const total = parseFloat(data.balance);
      const free = parseFloat(data.availableMargin);
      const upnl = parseFloat(data.unrealizedProfit);
      const used = parseFloat(data.usedMargin);

      return { total, free, used, upnl, info: data };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.balance;
    }
  };

  private _parsePrecision(precision: string | number) {
    const precisionNumber = parseInt(String(precision));
    let parsedPrecision = '0.';
    for (let i = 0; i < precisionNumber - 1; i++) {
      parsedPrecision = parsedPrecision + '0';
    }
    return parsedPrecision + '1';
  }

  fetchMarkets = async () => {
    try {
      const {
        data: { data: symbols },
      } = await this.xhr.get<Record<string, Array<Record<string, any>>>>(ENDPOINTS.MARKETS);

      const markets: Market[] = symbols.map((m) => {
        const baseAsset = v(m, 'asset');
        const quoteAsset = v(m, 'currency');

        return {
          id: m.symbol,
          symbol: m.symbol.replace(/-/g, ''),
          base: baseAsset,
          quote: quoteAsset,
          active: m.status === 1,
          precision: {
            amount: parseFloat(this._parsePrecision(parseFloat(v(m, 'quantityPrecision')))),
            price: parseFloat(this._parsePrecision(parseFloat(v(m, 'pricePrecision')))),
          },
          limits: {
            amount: {
              min: parseFloat(v(m, 'tradeMinLimit')),
              // TODO: get actual max order size
              max: Infinity,
            },
            leverage: {
              min: 1,
              // TODO: why is this sometimes 0? is there some default max lev for this case?
              max: v(m, 'maxLongLeverage') || 100,
            },
          },
        };
      });

      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickersRaw = async () => {
    try {
      const {
        data: { data: dailys },
      } = await this.xhr.get<Record<string, Array<Record<string, any>>>>(ENDPOINTS.TICKERS_24H);

      const {
        data: { data: prices },
      } = await this.xhr.get<Record<string, Array<Record<string, any>>>>(ENDPOINTS.TICKERS_PRICE);

      return { dailys, prices };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { dailys: [], prices: [] };
    }
  };

  formatTickers = ({ dailys, prices }: { dailys: any[]; prices: any[] }) => {
    return dailys.reduce((acc: Ticker[], daily) => {
      const market = this.store.marketsById[daily.symbol];
      const price = prices.find((p) => p.symbol === daily.symbol)!;

      if (!market || !daily || !price) return acc;

      const last = parseFloat(v(daily, 'lastPrice'));
      const touchPrices = this.deriveTickerBidAsk(market, last);
      const ticker = {
        id: market.id,
        symbol: market.symbol,
        bid: touchPrices.bid,
        ask: touchPrices.ask,
        last,
        mark: parseFloat(v(price, 'markPrice')),
        index: parseFloat(v(price, 'indexPrice')),
        percentage: parseFloat(v(daily, 'priceChangePercent')),
        fundingRate: parseFloat(v(price, 'lastFundingRate')) / 100,
        fundingTime: parseFloat(v(price, 'nextFundingTime')),
        volume: parseFloat(daily.volume),
        quoteVolume: parseFloat(v(daily, 'quoteVolume')),
        openInterest: 0, // Binance doesn't provides all tickers data
      };

      return [...acc, ticker];
    }, []);
  };

  fetchTickers = async () => {
    const { dailys, prices } = await this.fetchTickersRaw();
    if (this.isDisposed) return [];
    const tickers = this.formatTickers({ dailys, prices });
    this.store.update({ tickers });
    return tickers;
  };

  fetchPositions = async (symbols?: string[]) => {
    const openPositions = await this.fetchOpenPositions();
    if (!symbols) return openPositions;
    const openPositionsForSymbols = openPositions.filter((p) => symbols.includes(p.symbol));
    const missingSymbols = symbols.filter((s) => !openPositionsForSymbols.some((p) => p.symbol === s));
    const positionMetadatas = await this.fetchPositionMetadata(missingSymbols);
    const fakePositions = positionMetadatas.map((metadata) => {
      return {
        id: createPositionId(metadata.symbol, metadata.side, metadata.positionMode),
        ...metadata,
        entryPrice: 0,
        notional: 0,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
      };
    });
    return [...openPositionsForSymbols, ...fakePositions];
  };

  fetchOpenPositions = async (): Promise<Position[]> => {
    // hedged iso
    try {
      const { data } = await this.xhr.get<FETCH_POSITIONS_RESPONSE>(ENDPOINTS.POSITIONS);

      if (data.code !== 0) {
        this.emitAndLogError(data.msg);
        return this.store.positions;
      }

      // We need to filter out positions that corresponds to
      // markets that are not supported by safe-cex

      const supportedPositions: FETCH_POSITIONS_RESPONSE['data'] = data.data.filter((p) =>
        this.store.markets.some((m) => m.symbol === p.symbol.replace(/-/, '')),
      );

      const positionMode = await this.getOrFetchPositionMode();

      const positions: Position[] = supportedPositions.map((p) => {
        const entryPrice = parseFloat(v(p, 'avgPrice'));
        const contracts = parseFloat(v(p, 'positionAmt'));
        const upnl = parseFloat(v(p, 'unrealizedProfit'));
        const pSide = v(p, 'positionSide');
        const marginType = v(p, 'isolated') ? MarginType.Isolated : MarginType.Cross;

        // If account is not on hedge mode,
        // we need to define the side of the position with the contracts amount
        const side =
          (pSide in POSITION_SIDE && POSITION_SIDE[pSide]) || (contracts > 0 ? PositionSide.Long : PositionSide.Short);

        const symbol = p.symbol.replace(/-/, '');
        const notional = Math.abs(contracts) * entryPrice + upnl;
        const leverage = typeof p.leverage === 'number' ? p.leverage : parseFloat(p.leverage);
        const isolatedMargin = parseFloat(v(p, 'isolatedMargin') || v(p, 'initialMargin') || '0');
        const marginSize = isolatedMargin > 0 ? isolatedMargin : leverage > 0 ? notional / leverage : undefined;

        return {
          id: createPositionId(symbol, side, positionMode),
          symbol,
          side,
          entryPrice,
          notional,
          leverage,
          unrealizedPnl: upnl,
          contracts: Math.abs(contracts),
          liquidationPrice: parseFloat(v(p, 'liquidationPrice')),
          marginSize,
          marginType,
          positionMode,
          info: p,
        };
      });

      return positions;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.positions;
    }
  };

  fetchOrders = async () => {
    const { data } = await this.xhr.get<Record<string, Record<string, Array<Record<string, any>>>>>(
      ENDPOINTS.OPEN_ORDERS,
    );

    if (data.code) {
      this.emitAndLogError(`${data.msg}`);
      return this.store.orders;
    }

    const rawOrders = Array.isArray(data.data?.orders)
      ? data.data.orders
      : Array.isArray(data.data)
        ? data.data
        : [];

    const orders: Order[] = rawOrders.flatMap((o) => {
      const order = this.mapOrder(o);
      if (!order) return [];

      if (order.type === OrderType.StopLoss) {
        const ticker = this.store.tickersBySymbol[order.symbol];
        if (ticker && Number.isFinite(ticker.mark) && ticker.mark > 0) {
          if (
            (order.side === OrderSide.Buy && order.price < ticker.mark) ||
            (order.side === OrderSide.Sell && order.price > ticker.mark)
          ) {
            return [{ ...order, type: OrderType.TakeProfit, tpTriggerType: order.slTriggerType }];
          }
        }
      }
      return [order];
    });

    return orders;
  };

  mapOrder = (o: Record<string, any>): Order | undefined => {
    const id = o.orderId ?? o.orderID ?? o.i;
    const rawSymbol = o.symbol ?? o.s;
    const rawType = o.type ?? o.o;
    const rawSide = o.side ?? o.S;
    const rawStatusValue = o.status ?? o.X;
    const rawStatus = rawStatusValue === undefined ? undefined : String(rawStatusValue).toUpperCase();
    const orderType = ORDER_TYPE[String(rawType).toUpperCase()];
    const orderSide = ORDER_SIDE[String(rawSide).toUpperCase()];
    const symbol = typeof rawSymbol === 'string' ? rawSymbol.replace(/-/g, '') : undefined;
    const status =
      rawStatus === undefined ? OrderStatus.Open : ORDER_STATUS[rawStatus as keyof typeof ORDER_STATUS] ?? undefined;

    if (id === undefined || !symbol || !orderType || !orderSide || !status) {
      return undefined;
    }

    const close = o.closePosition === true || o.closePosition === 'true';
    const rawAmount = o.origQty ?? o.quantity ?? o.q;
    const price = Number(o.price ?? o.p);
    const stopPrice = Number(o.stopPrice ?? o.sp);
    const amount = rawAmount === undefined && close ? 0 : Number(rawAmount);
    const filled = Number(o.executedQty ?? o.z ?? 0);
    if (!Number.isFinite(amount) || !Number.isFinite(filled)) {
      return undefined;
    }

    const existingOrder = this.store.orders.find((order) => order.id === String(id));
    const reduceOnlyValue = o.reduceOnly ?? o.ro ?? o.R;
    const postOnlyValue = o.postOnly;
    const rawTimeInForce = o.timeInForce ?? o.f;
    const timeInForce =
      postOnlyValue === true || postOnlyValue === 'true'
        ? OrderTimeInForce.PostOnly
        : TIME_IN_FORCE[rawTimeInForce] ??
          (rawTimeInForce === undefined ? existingOrder?.timeInForce : OrderTimeInForce.GoodTillCancel);

    return {
      id: String(id),
      status,
      symbol,
      type: orderType,
      side: orderSide,
      price: Number.isFinite(price) && price > 0 ? price : Number.isFinite(stopPrice) ? stopPrice : 0,
      amount,
      reduceOnly: reduceOnlyValue === true || reduceOnlyValue === 'true',
      close,
      timeInForce,
      filled,
      lastUpdate: virtualClock.getCurrentTimeMs(),
      remaining: subtract(amount, filled),
      info: o,
      ...(orderType === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[o.workingType ?? o.wt] }),
      ...(orderType === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[o.workingType ?? o.wt] }),
    };
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const limit = Math.min(opts.limit || 500, 1500);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const market = this.store.marketsBySymbol[opts.symbol];

    if (market) {
      try {
        const { data: response } = await this.xhr.get<{ code?: number; msg?: string; data?: any[] }>(ENDPOINTS.KLINE, {
          params: {
            symbol: market.id,
            interval,
            startTime: start.valueOf(),
            endTime: end.valueOf(),
            // ...(opts.limit
            //   ? {
            //       limit,
            //     }
            //   : {}),
          },
        });

        if (response.code !== undefined && response.code !== 0) {
          this.log(`BingX kline request failed: ${response.msg || `code ${response.code}`}`, LogSeverity.Warning);
          return [];
        }

        const data = response.data;
        if (!Array.isArray(data)) {
          this.log(`BingX kline response did not include candles for ${opts.symbol}`, LogSeverity.Warning);
          return [];
        }

        const candles: Candle[] = data.map(({ time, open, high, low, close, volume }: any) => {
          return {
            timestamp: time / 1000,
            open: parseFloat(open),
            high: parseFloat(high),
            low: parseFloat(low),
            close: parseFloat(close),
            volume: parseFloat(volume),
          };
        });

        candles.sort((a, b) => a.timestamp - b.timestamp);

        return candles;
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        return [];
      }
    }

    this.emitAndLogError(`Market ${opts.symbol} not found on BingX`);
    return [];
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      const { data } = await this.xhr.get<FETCH_LEVERAGE_RESPONSE>(ENDPOINTS.LEVERAGE, {
        params: {
          symbol: this.toBingxSymbol(symbol),
        },
      });

      if (data.code !== 0) {
        throw new Error(data.msg);
      }

      const leverageObj: FetchedLeverage = {
        buyLeverage: data.data.longLeverage,
        sellLeverage: data.data.shortLeverage,
      };

      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: leverageObj,
        },
      });

      return leverageObj;
    } catch (err: any) {
      let errorMessage: string;
      if (err?.response?.data?.msg) {
        errorMessage = err.response.data.msg;
      } else if (err?.message) {
        errorMessage = err.message;
      } else if (typeof err === 'object') {
        errorMessage = JSON.stringify(err, null, 2);
      } else {
        errorMessage = String(err);
      }
      this.emitAndLogError(errorMessage, 'Failed to fetch leverage');
      return null;
    }
  };

  fetchMarginModeForSymbol = async (symbol: string) => {
    try {
      const { data } = await this.xhr.get<FETCH_MARGIN_MODE_RESPONSE>(ENDPOINTS.MARGIN_MODE, {
        params: {
          symbol: this.toBingxSymbol(symbol),
        },
      });

      if (data.code !== 0) {
        throw new Error(data.msg);
      }

      return data.data.marginType === 'ISOLATED' ? MarginType.Isolated : MarginType.Cross;
    } catch (err: any) {
      let errorMessage: string;
      if (err?.response?.data?.msg) {
        errorMessage = err.response.data.msg;
      } else if (err?.message) {
        errorMessage = err.message;
      } else if (typeof err === 'object') {
        errorMessage = JSON.stringify(err, null, 2);
      } else {
        errorMessage = String(err);
      }
      this.emitAndLogError(errorMessage);
      throw err;
    }
  };

  getOrFetchPositionMode = async (refetch = false): Promise<PositionMode> => {
    if (this._accountPositionMode && !refetch) return this._accountPositionMode;
    const { data } = await this.xhr.get<FETCH_HEDGE_MODE_RESPONSE>(ENDPOINTS.HEDGE_MODE);
    // {
    //   "code": 0,
    //   "msg": "",
    //   "timeStamp": 1714546566486,
    //   "data": {
    //   "dualSidePosition": "false"
    // }
    // }
    if (data.code !== 0) {
      this.emitAndLogError(data.msg);
      throw new Error(data.msg);
    }
    this._accountPositionMode = data.data.dualSidePosition === 'true' ? PositionMode.Hedged : PositionMode.OneWay;
    return this._accountPositionMode;
  };

  fetchPositionMetadata = async (
    symbols: string[],
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
    try {
      const positionMode = await this.getOrFetchPositionMode();
      return flatten(
        await mapSeries(
          symbols,
          async (symbol): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
            if (this.isDisposed) return [];

            const [leverage, marginType] = await Promise.all([
              this.fetchLeverageForSymbol(symbol),
              this.fetchMarginModeForSymbol(symbol),
            ]);
            if (this.isDisposed) return [];

            // If leverage fetch failed, skip this symbol
            if (!leverage) return [];

            if (positionMode === PositionMode.OneWay) {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage: leverage.buyLeverage,
                  positionMode,
                  marginType,
                },
              ];
            } else {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage: leverage.buyLeverage,
                  positionMode,
                  marginType,
                },
                {
                  symbol,
                  side: PositionSide.Short,
                  leverage: leverage.sellLeverage,
                  positionMode,
                  marginType,
                },
              ];
            }
          },
        ),
      );
    } catch (err: any) {
      let errorMessage: string;
      if (err?.response?.data?.msg) {
        errorMessage = err.response.data.msg;
      } else if (err?.message) {
        errorMessage = err.message;
      } else if (typeof err === 'object') {
        errorMessage = JSON.stringify(err, null, 2);
      } else {
        errorMessage = String(err);
      }
      this.emitAndLogError(errorMessage, 'Failed to fetch order history');
      return [];
    }
  };

  changePositionMode = async (hedged: boolean) => {
    try {
      const { data } = await this.xhr.post(ENDPOINTS.HEDGE_MODE, {
        dualSidePosition: hedged ? 'true' : 'false',
      });
      // {
      //   "code": 109401,
      //   "msg": "user has pending orders",
      //   "data": {}
      // }
      if (data.code !== 0) {
        throw new Error(data.msg);
      }
      await this.getOrFetchPositionMode(true);
      const positions = await this.fetchPositions();
      this.store.update({ positions });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }
    try {
      const { data } = await this.xhr.post(ENDPOINTS.MARGIN_MODE, {
        symbol: this.toBingxSymbol(symbol),
        marginType: isolated ? 'ISOLATED' : 'CROSSED',
      });
      // {
      //   "code": 109401,
      //   "msg": "user has pending orders",
      //   "data": {}
      // }
      if (data.code !== 0) {
        throw new Error(data.msg);
      }
      await this.getOrFetchPositionMode(true);
      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number, side?: PositionSide) => {
    try {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) throw new Error(`Market ${symbol} not found`);

      const positionMode = await this.getOrFetchPositionMode();
      let positionSide = 'BOTH';
      if (positionMode === PositionMode.Hedged) {
        if (!side) {
          throw new Error('Side is required when position mode is hedged');
        }
        if (side === PositionSide.Long) {
          positionSide = 'LONG';
        } else {
          positionSide = 'SHORT';
        }
      }

      const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

      const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
        symbol: this.toBingxSymbol(symbol),
        leverage,
        side: positionSide,
      });

      if (data.code !== 0) {
        throw new Error(data.msg);
      }

      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      let errorMessage: string;
      if (err?.response?.data?.msg) {
        errorMessage = err.response.data.msg;
      } else if (err?.message) {
        errorMessage = err.message;
      } else if (typeof err === 'object') {
        errorMessage = JSON.stringify(err, null, 2);
      } else {
        errorMessage = String(err);
      }
      this.emitAndLogError(errorMessage);
      throw err;
    }
  };

  cancelOrders = async (orders: Order[]) => {
    try {
      const groupedBySymbol = groupBy(orders, 'symbol');
      const requests = Object.entries(groupedBySymbol).map(([symbol, symbolOrders]) => {
        return {
          symbol: this.toBingxSymbol(symbol),
          orderIdList: symbolOrders.map((o) => o.id),
        };
      });

      await forEachSeries(requests, async (request) => {
        const canceledOrderIds: string[] = [];
        // Cancel orders one by one - batch cancellation disabled due to signature issues
        for (const orderId of request.orderIdList) {
          try {
            const { data } = await this.xhr.delete(ENDPOINTS.ORDER, {
              params: {
                symbol: request.symbol,
                orderId: orderId,
              },
            });
            // {
            //   "code": 80016,
            //   "msg": "order not exist",
            //   "data": {}
            // }
            if (data.code === 0 || data.code === 80016) {
              canceledOrderIds.push(orderId);
            } else {
              this.emitAndLogError(`Failed to cancel order ${orderId}: ${data.msg}`);
            }
          } catch (err: any) {
            this.emitAndLogError(`Failed to cancel order ${orderId}: ${err?.response?.data?.msg || err?.message}`);
          }
        }

        if (canceledOrderIds.length) {
          this.store.updateOrdersStatus(
            canceledOrderIds.map((id) => ({ id })),
            OrderStatus.Canceled,
          );
        }
      });
    } catch (err: any) {
      // Extract error message properly
      let errorMessage: string;
      if (err?.response?.data?.msg) {
        errorMessage = err.response.data.msg;
      } else if (err?.message) {
        errorMessage = err.message;
      } else if (typeof err === 'object') {
        // Properly stringify objects to avoid [object Object]
        errorMessage = JSON.stringify(err, null, 2);
      } else {
        errorMessage = String(err);
      }

      this.emitAndLogError(errorMessage);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      await this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
        params: { symbol: this.toBingxSymbol(symbol) },
      });

      this.store.updateOrdersStatus(
        this.store.orders.filter((o) => o.symbol === symbol),
        OrderStatus.Canceled,
      );
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    if (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit) {
      if ('amount' in update) {
        return rejectPreflight('BingX trigger order movement supports price updates only');
      }
      return await this.replaceTriggerOrderPrice(order, update.price);
    }
    if (order.status !== OrderStatus.Open || order.type !== OrderType.Limit) {
      return rejectPreflight('BingX native amendment supports only exactly reconciled open LIMIT orders');
    }
    if ('price' in update && 'amount' in update) {
      return rejectPreflight('BingX limit price replacement cannot be combined with native quantity amendment');
    }
    if ('price' in update) {
      return await this.cancelReplaceLimitOrderPrice(order, update.price);
    }
    if (!('amount' in update)) {
      return rejectPreflight('BingX native amendment requires an amount update');
    }
    if (
      order.timeInForce !== OrderTimeInForce.GoodTillCancel &&
      order.timeInForce !== OrderTimeInForce.PostOnly
    ) {
      return rejectPreflight('BingX native amendment requires a reconciled GTC or PostOnly time in force');
    }
    if (order.close) {
      return rejectPreflight('BingX native amendment does not support close-position orders');
    }

    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) {
      return rejectPreflight(`Market ${order.symbol} not found`);
    }

    const info = order.info;
    const rawOrderId = info?.orderId ?? info?.i;
    const rawSymbol = info?.symbol ?? info?.s;
    const rawType = String(info?.type ?? info?.o ?? '').toUpperCase();
    const rawSide = String(info?.side ?? info?.S ?? '').toUpperCase();
    const rawPositionSide = String(info?.positionSide ?? info?.ps ?? '').toUpperCase();
    const rawStatus = String(info?.status ?? info?.X ?? '').toUpperCase();
    const rawQuantity = Number(info?.origQty ?? info?.quantity ?? info?.q);
    const rawExecutedQuantity = Number(info?.executedQty ?? info?.z);
    const rawReduceOnlyValue = info?.reduceOnly ?? info?.ro ?? info?.R;
    const rawReduceOnly =
      rawReduceOnlyValue === true || rawReduceOnlyValue === 'true'
        ? true
        : rawReduceOnlyValue === false || rawReduceOnlyValue === 'false'
          ? false
          : undefined;

    if (
      rawOrderId === undefined ||
      String(rawOrderId) !== order.id ||
      typeof rawSymbol !== 'string' ||
      rawSymbol.replace(/-/g, '') !== order.symbol ||
      rawType !== 'LIMIT' ||
      ORDER_SIDE[rawSide] !== order.side ||
      !['BOTH', 'LONG', 'SHORT'].includes(rawPositionSide)
    ) {
      return rejectPreflight(`BingX cannot safely amend ${order.id}: immutable order identity is not reconciled`);
    }
    if (rawReduceOnly !== undefined && rawReduceOnly !== order.reduceOnly) {
      return rejectPreflight(`BingX cannot safely amend ${order.id}: reduce-only state is not reconciled`);
    }

    const rawPostOnlyValue = info?.postOnly;
    const rawPostOnly =
      rawPostOnlyValue === true || rawPostOnlyValue === 'true'
        ? true
        : rawPostOnlyValue === false || rawPostOnlyValue === 'false'
          ? false
          : undefined;
    const rawTimeInForceValue = String(info?.timeInForce ?? info?.f ?? '');
    const parsedRawTimeInForce = TIME_IN_FORCE[rawTimeInForceValue];
    const rawTimeInForce = rawPostOnly === true ? OrderTimeInForce.PostOnly : parsedRawTimeInForce;
    if (
      parsedRawTimeInForce === undefined ||
      rawTimeInForce === undefined ||
      rawTimeInForce !== order.timeInForce
    ) {
      return rejectPreflight(`BingX cannot safely amend ${order.id}: time in force is not reconciled`);
    }

    const adjustedOrderAmount = adjust(order.amount, market.precision.amount);
    const adjustedRemaining = adjust(order.remaining, market.precision.amount);
    const adjustedRawQuantity = adjust(rawQuantity, market.precision.amount);
    if (
      !['NEW', 'OPEN', 'PENDING', 'WORKING'].includes(rawStatus) ||
      order.filled !== 0 ||
      rawExecutedQuantity !== 0 ||
      adjustedRemaining !== adjustedOrderAmount ||
      adjustedRawQuantity !== adjustedOrderAmount
    ) {
      return rejectPreflight(
        'BingX partially filled amendment is disabled until quantity is documented as total or remaining size',
      );
    }

    const hasAttachedProtection = (value: unknown) => {
      if (!value || typeof value !== 'object') return false;
      const protection = value as Record<string, unknown>;
      return Number(protection.stopPrice) > 0 || Number(protection.quantity) > 0 || Number(protection.price) > 0;
    };
    if (
      order.takeProfit !== undefined ||
      order.stopLoss !== undefined ||
      hasAttachedProtection(info?.takeProfit) ||
      hasAttachedProtection(info?.stopLoss)
    ) {
      return rejectPreflight('BingX native amendment cannot prove attached TP/SL quantity preservation');
    }

    const adjustedQuantity = adjust(update.amount, market.precision.amount);
    if (!Number.isFinite(adjustedQuantity) || adjustedQuantity <= 0) {
      return rejectPreflight('BingX amended order quantity must be greater than zero');
    }
    if (adjustedQuantity < market.limits.amount.min || adjustedQuantity > market.limits.amount.max) {
      return rejectPreflight('BingX amended order quantity is outside the reconciled market limits');
    }
    if (adjustedQuantity === adjustedOrderAmount) {
      return rejectPreflight('BingX amended order quantity must change after market-precision normalization');
    }

    try {
      const { data } = await this.xhr.post(ENDPOINTS.AMEND_ORDER, {
        orderId: order.id,
        quantity: adjustedQuantity,
        symbol: market.id,
      });
      if (data.code !== 0) {
        throw new Error(data.msg || `BingX amend order failed with code ${data.code}`);
      }
      if (
        String(data?.data?.orderId ?? '') !== order.id ||
        String(data?.data?.symbol ?? '').replace(/-/g, '') !== order.symbol ||
        adjust(Number(data?.data?.quantity), market.precision.amount) !== adjustedQuantity
      ) {
        throw new Error(`BingX amend response did not confirm order ${order.id}`);
      }

      return [order.id];
    } catch (error: any) {
      const message = error?.response?.data?.msg || error?.message || String(error);
      this.emitAndLogError(message);
      throw error;
    }
  };

  private replaceTriggerOrderPrice = async (order: Order, nextPrice: number): Promise<string[]> => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    if (
      order.status !== OrderStatus.Open ||
      (order.type !== OrderType.StopLoss && order.type !== OrderType.TakeProfit)
    ) {
      return rejectPreflight('BingX trigger replacement supports only reconciled open STOP/TP orders');
    }

    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) {
      return rejectPreflight(`Market ${order.symbol} not found`);
    }

    const info = order.info;
    const rawSymbol = info?.symbol ?? info?.s;
    const rawType = String(info?.type ?? info?.o ?? '').toUpperCase();
    const rawSide = String(info?.side ?? info?.S ?? '').toUpperCase();
    const rawStatus = String(info?.status ?? info?.X ?? '').toUpperCase();
    const rawExecutedQuantity = Number(info?.executedQty ?? info?.z ?? 0);
    const mappedRawType = ORDER_TYPE[rawType];
    const mappedRawSide = ORDER_SIDE[rawSide];
    const rawTriggerType =
      mappedRawType === OrderType.StopLoss || mappedRawType === OrderType.TakeProfit ? mappedRawType : undefined;
    const replacementType = rawTriggerType ?? order.type;
    if (typeof rawSymbol === 'string' && rawSymbol.replace(/-/g, '') !== order.symbol) {
      return rejectPreflight(`BingX cannot safely replace ${order.id}: trigger order identity is not reconciled`);
    }
    if (
      (rawType && !rawTriggerType) ||
      (rawSide && mappedRawSide !== order.side) ||
      (rawStatus && !['NEW', 'OPEN', 'PENDING', 'WORKING'].includes(rawStatus)) ||
      order.filled !== 0 ||
      rawExecutedQuantity !== 0
    ) {
      return rejectPreflight('BingX trigger replacement is disabled for partially filled orders');
    }

    const adjustedOrderAmount = adjust(order.amount, market.precision.amount);
    if (!order.close && (!Number.isFinite(adjustedOrderAmount) || adjustedOrderAmount <= 0)) {
      return rejectPreflight('BingX replacement trigger quantity must be greater than zero');
    }

    const adjustedPrice = adjust(nextPrice, market.precision.price);
    if (!Number.isFinite(adjustedPrice) || adjustedPrice <= 0) {
      return rejectPreflight('BingX replacement trigger price must be greater than zero');
    }
    if (adjustedPrice === adjust(order.price, market.precision.price)) {
      return rejectPreflight('BingX replacement trigger price must change after market-precision normalization');
    }

    const ticker = this.store.tickersBySymbol[order.symbol];
    if (ticker && Number.isFinite(ticker.mark) && ticker.mark > 0) {
      const wouldTriggerImmediately =
        replacementType === OrderType.StopLoss
          ? (order.side === OrderSide.Sell && adjustedPrice >= ticker.mark) ||
            (order.side === OrderSide.Buy && adjustedPrice <= ticker.mark)
          : (order.side === OrderSide.Sell && adjustedPrice <= ticker.mark) ||
            (order.side === OrderSide.Buy && adjustedPrice >= ticker.mark);
      if (wouldTriggerImmediately) {
        return rejectPreflight('BingX replacement trigger price would execute immediately against current mark price');
      }
    }

    await this.cancelOrders([order]);
    return await this.placeOrder({
      symbol: order.symbol,
      type: replacementType,
      side: order.side,
      amount: adjustedOrderAmount,
      price: adjustedPrice,
      reduceOnly: order.reduceOnly,
      close: order.close,
      tpTriggerType:
        replacementType === OrderType.TakeProfit ? (order.tpTriggerType ?? order.slTriggerType) : undefined,
      slTriggerType: replacementType === OrderType.StopLoss ? (order.slTriggerType ?? order.tpTriggerType) : undefined,
    });
  };

  private cancelReplaceLimitOrderPrice = async (order: Order, nextPrice: number): Promise<string[]> => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    if (order.status !== OrderStatus.Open || order.type !== OrderType.Limit) {
      return rejectPreflight('BingX cancel-replace supports only exactly reconciled open LIMIT orders');
    }
    if (order.close) {
      return rejectPreflight('BingX cancel-replace does not support close-position orders');
    }

    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) {
      return rejectPreflight(`Market ${order.symbol} not found`);
    }

    let orderToReplace = order;
    if (!orderToReplace.timeInForce) {
      const reconciledOrder = (await this.fetchOrders()).find((candidate) => candidate.id === order.id);
      if (reconciledOrder?.timeInForce) {
        orderToReplace = reconciledOrder;
      }
    }

    const info = orderToReplace.info;
    const rawOrderId = info?.orderId ?? info?.orderID ?? info?.i;
    const rawSymbol = info?.symbol ?? info?.s;
    const rawType = String(info?.type ?? info?.o ?? '').toUpperCase();
    const rawSide = String(info?.side ?? info?.S ?? '').toUpperCase();
    const rawPositionSide = String(info?.positionSide ?? info?.ps ?? '').toUpperCase();
    const rawStatus = String(info?.status ?? info?.X ?? '').toUpperCase();
    const rawQuantity = Number(info?.origQty ?? info?.quantity ?? info?.q);
    const rawExecutedQuantity = Number(info?.executedQty ?? info?.z);
    const rawClientOrderId = info?.clientOrderId ?? info?.clientOrderID ?? info?.c;
    const rawReduceOnlyValue = info?.reduceOnly ?? info?.ro ?? info?.R;
    const rawReduceOnly =
      rawReduceOnlyValue === true || rawReduceOnlyValue === 'true'
        ? true
        : rawReduceOnlyValue === false || rawReduceOnlyValue === 'false'
          ? false
          : undefined;
    const mappedRawSide = ORDER_SIDE[rawSide];
    const resolvedPositionSide = ['BOTH', 'LONG', 'SHORT'].includes(rawPositionSide)
      ? rawPositionSide
      : await this.getOrderPositionSide({
          symbol: order.symbol,
          side: orderToReplace.side,
          type: orderToReplace.type,
          amount: orderToReplace.amount,
          price: orderToReplace.price,
          reduceOnly: orderToReplace.reduceOnly,
        });

    if (
      (rawOrderId !== undefined && String(rawOrderId) !== orderToReplace.id) ||
      (typeof rawSymbol === 'string' && rawSymbol.replace(/-/g, '') !== orderToReplace.symbol) ||
      (rawType && rawType !== 'LIMIT') ||
      (rawSide && mappedRawSide !== orderToReplace.side) ||
      (rawPositionSide && !['BOTH', 'LONG', 'SHORT'].includes(rawPositionSide))
    ) {
      return rejectPreflight(`BingX cannot safely replace ${order.id}: immutable order identity is not reconciled`);
    }
    if (rawReduceOnly !== undefined && rawReduceOnly !== orderToReplace.reduceOnly) {
      return rejectPreflight(`BingX cannot safely replace ${order.id}: reduce-only state is not reconciled`);
    }

    const rawPostOnlyValue = info?.postOnly;
    const rawPostOnly =
      rawPostOnlyValue === true || rawPostOnlyValue === 'true'
        ? true
        : rawPostOnlyValue === false || rawPostOnlyValue === 'false'
          ? false
          : undefined;
    const rawTimeInForceValue = String(info?.timeInForce ?? info?.f ?? '');
    const parsedRawTimeInForce = TIME_IN_FORCE[rawTimeInForceValue];
    const rawTimeInForce =
      rawPostOnly === true ? OrderTimeInForce.PostOnly : (parsedRawTimeInForce ?? orderToReplace.timeInForce);
    if (
      rawTimeInForce === undefined ||
      (rawPostOnly === false && parsedRawTimeInForce === undefined && orderToReplace.timeInForce === undefined) ||
      rawTimeInForce !== orderToReplace.timeInForce
    ) {
      return rejectPreflight(`BingX cannot safely replace ${order.id}: time in force is not reconciled`);
    }

    const adjustedOrderAmount = adjust(orderToReplace.amount, market.precision.amount);
    const adjustedRemaining = adjust(orderToReplace.remaining, market.precision.amount);
    const resolvedRawQuantity = Number.isFinite(rawQuantity) ? rawQuantity : orderToReplace.amount;
    const resolvedRawExecutedQuantity = Number.isFinite(rawExecutedQuantity) ? rawExecutedQuantity : orderToReplace.filled;
    const adjustedRawQuantity = adjust(resolvedRawQuantity, market.precision.amount);
    if (
      (rawStatus && !['NEW', 'OPEN', 'PENDING', 'WORKING'].includes(rawStatus)) ||
      orderToReplace.filled !== 0 ||
      resolvedRawExecutedQuantity !== 0 ||
      adjustedRemaining !== adjustedOrderAmount ||
      adjustedRawQuantity !== adjustedOrderAmount
    ) {
      return rejectPreflight('BingX cancel-replace is disabled for partially filled LIMIT orders');
    }

    const hasAttachedProtection = (value: unknown) => {
      if (!value || typeof value !== 'object') return false;
      const protection = value as Record<string, unknown>;
      return Number(protection.stopPrice) > 0 || Number(protection.quantity) > 0 || Number(protection.price) > 0;
    };
    if (
      orderToReplace.takeProfit !== undefined ||
      orderToReplace.stopLoss !== undefined ||
      hasAttachedProtection(info?.takeProfit) ||
      hasAttachedProtection(info?.stopLoss)
    ) {
      return rejectPreflight('BingX cancel-replace cannot prove attached TP/SL preservation');
    }

    const adjustedPrice = adjust(nextPrice, market.precision.price);
    if (!Number.isFinite(adjustedPrice) || adjustedPrice <= 0) {
      return rejectPreflight('BingX replacement order price must be greater than zero');
    }
    if (adjustedPrice === adjust(orderToReplace.price, market.precision.price)) {
      return rejectPreflight('BingX replacement order price must change after market-precision normalization');
    }

    const timeInForce = inverseObj(TIME_IN_FORCE)[orderToReplace.timeInForce];
    if (!timeInForce) {
      return rejectPreflight(`BingX cannot safely replace ${order.id}: time in force is not reconciled`);
    }
    if (typeof rawClientOrderId !== 'string' || rawClientOrderId.length === 0) {
      return rejectPreflight(`BingX cannot safely replace ${order.id}: client order id is not reconciled`);
    }

    const cancelRestrictions =
      rawStatus === 'PENDING'
        ? 'ONLY_PENDING'
        : rawStatus === 'NEW'
          ? 'ONLY_NEW'
          : undefined;
    const payload = omitUndefined({
      symbol: market.id,
      cancelClientOrderId: rawClientOrderId,
      side: inverseObj(ORDER_SIDE)[orderToReplace.side],
      positionSide: resolvedPositionSide,
      type: 'LIMIT',
      quantity: adjustedOrderAmount,
      price: adjustedPrice,
      cancelReplaceMode: 'STOP_ON_FAILURE',
      cancelRestrictions,
      timeInForce,
      clientOrderId: this.createClientOrderId(),
      reduceOnly: resolvedPositionSide === 'BOTH' && orderToReplace.reduceOnly ? true : undefined,
    });

    try {
      const { data } = await this.xhr.post(ENDPOINTS.CANCEL_REPLACE_ORDER, payload);
      if (data.code !== 0) {
        throw new Error(data.msg || `BingX cancel-replace failed with code ${data.code}`);
      }

      const result = data?.data || {};
      const isSuccessfulResult = (value: unknown) => value === true || ['SUCCESS', 'TRUE'].includes(String(value).toUpperCase());
      const cancelResult = result.cancelResult;
      const replaceResult = result.newOrderResult ?? result.replaceResult;
      if (!isSuccessfulResult(cancelResult) || !isSuccessfulResult(replaceResult)) {
        throw new Error(
          `BingX cancel-replace failed: cancel=${cancelResult || 'unknown'}, new=${replaceResult || 'unknown'}`,
        );
      }

      const newOrderResponse = result.newOrderResponse || {};
      const newOrderId = result.newOrderId ?? newOrderResponse.orderId ?? newOrderResponse.orderID ?? result.orderId ?? result.orderID;
      if (newOrderId === undefined) {
        throw new Error(`BingX cancel-replace response did not include a replacement order id`);
      }

      const replacementOrder = this.mapOrder({
        ...newOrderResponse,
        ...info,
        ...payload,
        orderId: String(newOrderId),
        orderID: String(newOrderId),
        status: 'PENDING',
        origQty: String(adjustedOrderAmount),
        executedQty: '0',
      });
      this.store.updateOrderStatus({ id: orderToReplace.id }, OrderStatus.Canceled);
      if (replacementOrder) {
        this.store.addOrUpdateOrder(replacementOrder);
      }

      return [String(newOrderId)];
    } catch (error: any) {
      const message = error?.response?.data?.msg || error?.message || String(error);
      this.emitAndLogError(message);
      throw error;
    }
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    const payloads = await this.formatCreateOrder(opts);
    return await this.placeOrderBatch(payloads);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    // prefetch position mode to prevent dispatching request for every order
    await this.getOrFetchPositionMode();
    const payloads = (await Promise.all(orders.flatMap(this.formatCreateOrder.bind(this)))).flat();
    return await this.placeOrderBatch(payloads);
  };

  private formatCreateOrder = async (opts: PlaceOrderOpts) => {
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    if (opts.type === OrderType.TrailingStopLoss) {
      return await this.formatCreateTrailingStopLossOrder(opts);
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const existingPosition = this.store.positions.find((p) => p.symbol === opts.symbol);

    let positionMode = existingPosition?.positionMode ?? (await this.getOrFetchPositionMode());
    if (this.isLikelyTradfiMarket(market) && positionMode === PositionMode.OneWay) {
      // Position mode can change outside the app, so re-check before failing fast.
      positionMode = await this.getOrFetchPositionMode(true);
      if (positionMode === PositionMode.OneWay) {
        throw new Error(this.buildTradfiOneWayModeError(opts.symbol));
      }
    }

    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

    const pSide = await this.getOrderPositionSide(opts, positionMode);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);
    const wantsFullPositionStop = isStopOrTP && (opts.close || !amount);

    // We use price only for limit orders
    // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    // Binance stopPrice only for SL or TP orders
    const priceField = isStopOrTP ? 'stopPrice' : 'price';

    const reduceOnly = positionMode === PositionMode.OneWay && opts.reduceOnly;
    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    let ordType = inverseObj(ORDER_TYPE)[opts.type];
    if (isStopOrTP) {
      const oppositePosSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
      const foundPosition = this.store.positions.find(
        (p) =>
          p.symbol === opts.symbol && p.side === oppositePosSide && p.contracts > 0 && p.positionMode === positionMode,
      );
      if (!foundPosition) {
        if (wantsFullPositionStop) {
          const msg = 'Cannot place a full-position stop loss or take profit order without an open position';
          this.emitAndLogError(msg);
          throw new Error(msg);
        }
        // needs to be trigger market if position isnt open
        ordType = 'TRIGGER_MARKET';
      }
    }

    const req = omitUndefined({
      symbol: market.id,
      positionSide: pSide,
      side: inverseObj(ORDER_SIDE)[opts.side],
      type: ordType,
      quantity: wantsFullPositionStop ? undefined : amount ? +amount : undefined,
      [priceField]: price,
      timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
      closePosition: wantsFullPositionStop ? 'true' : undefined,
      reduceOnly:
        wantsFullPositionStop || positionMode === PositionMode.Hedged
          ? undefined
          : ordType === 'TRIGGER_MARKET'
            ? true
            : reduceOnly && !isStopOrTP
              ? true
              : undefined,
      ...(opts.type === OrderType.TakeProfit && {
        workingType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'CONTRACT_PRICE',
      }),
      ...(opts.type === OrderType.StopLoss && {
        workingType: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'CONTRACT_PRICE' : 'MARK_PRICE',
      }),
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);

    const isOrdinaryOrder = opts.type === OrderType.Market || opts.type === OrderType.Limit;
    const withClientOrderId = (payload: Record<string, any>) =>
      isOrdinaryOrder ? { ...payload, clientOrderId: callerClientOrderId ?? this.createClientOrderId() } : payload;

    const payloads: Array<Record<string, any>> = wantsFullPositionStop
      ? [req]
      : times(lots, () =>
          withClientOrderId({
            ...req,
            quantity: lotSize,
          }),
        );

    if (rest) {
      payloads.push(withClientOrderId({ ...req, quantity: rest }));
    }
    this.assertSingleNativeClientOrderId(callerClientOrderId, payloads.length);

    if (opts.stopLoss) {
      // For attached SL/TP with entry orders, use the entry order size
      payloads.push({
        ...omit(req, 'price'),
        side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
        // needs to be trigger market bc position likely isnt open
        type: 'TRIGGER_MARKET',
        stopPrice: adjust(opts.stopLoss, pPrice),
        quantity: amount ? +amount : undefined,
        timeInForce: 'GTC',
        reduceOnly: positionMode === PositionMode.Hedged ? undefined : true,
      });
    }

    if (opts.takeProfit) {
      // For attached TP with entry orders, use the entry order size
      payloads.push({
        ...omit(req, 'price'),
        side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
        // needs to be trigger market bc position likely isnt open
        type: 'TRIGGER_MARKET',
        stopPrice: adjust(opts.takeProfit, pPrice),
        quantity: amount ? +amount : undefined,
        timeInForce: 'GTC',
        reduceOnly: positionMode === PositionMode.Hedged ? undefined : true,
      });
    }

    return payloads;
  };

  private formatCreateTrailingStopLossOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickersBySymbol[opts.symbol];

    const pSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;

    const position = this.store.positions.find((p) => p.symbol === opts.symbol && p.side === pSide);

    if (!market) {
      const message = `Market ${opts.symbol} not found`;
      this.emitAndLogError(message);
      throw new Error(message);
    }
    if (!ticker || !Number.isFinite(ticker.last) || ticker.last <= 0) {
      const message = `Ticker ${opts.symbol} not found`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (!position) {
      const message = `Position ${opts.symbol} and side ${pSide} not found`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (!Number.isFinite(opts.price)) {
      const message = `Trailing stop price for ${opts.symbol} is required`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const priceDistance = adjust(
      Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
      market.precision.price,
    );

    const priceRate = Math.round((priceDistance / ticker.last) * 10_000) / 10_000;
    if (priceRate <= 0 || priceRate > 1) {
      const message = `BingX trailing stop callback rate must be greater than 0 and at most 1`;
      this.emitAndLogError(message);
      throw new Error(message);
    }
    const positionSide = await this.getOrderPositionSide(opts, position?.positionMode);

    const payload = {
      symbol: market.id,
      positionSide,
      side: inverseObj(ORDER_SIDE)[opts.side],
      type: inverseObj(ORDER_TYPE)[OrderType.TrailingStopLoss],
      quantity: `${position.contracts}`,
      priceRate: `${priceRate}`,
      priceProtect: 'true',
    };

    return [payload];
  };

  private getOrderPositionSide = async (opts: PlaceOrderOpts, overridePositionMode?: PositionMode) => {
    let positionSide = 'BOTH';

    const positionMode = overridePositionMode ?? (await this.getOrFetchPositionMode());

    // We need to specify side of the position to interract with
    // if we are in hedged mode on the bingx account
    if (positionMode === PositionMode.Hedged) {
      positionSide = opts.side === OrderSide.Buy ? 'LONG' : 'SHORT';

      if (
        opts.type === OrderType.StopLoss ||
        opts.type === OrderType.TakeProfit ||
        opts.type === OrderType.TrailingStopLoss ||
        opts.reduceOnly
      ) {
        positionSide = positionSide === 'LONG' ? 'SHORT' : 'LONG';
      }
    }

    return positionSide;
  };

  private placeOrderBatchOld = async (payloads: any[]) => {
    const lots = chunk(payloads, 5);
    const orderIds = [] as string[];
    let errMsg = '';

    for (const lot of lots) {
      if (lot.length === 1) {
        const results = await Promise.all(
          lot.map(async (p) => {
            try {
              const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, p);
              if (data.code !== 0) {
                errMsg = data.msg;
                this.emitAndLogError(errMsg);
              } else {
                return data?.data?.order?.orderID || data?.data?.order?.orderId;
              }
            } catch (err: any) {
              if (err?.response?.data?.msg) {
                errMsg = err.response.data.msg;
              } else if (err?.response?.msg) {
                errMsg = err.response.msg;
              } else if (err?.msg) {
                errMsg = err.msg;
              } else if (err?.message) {
                errMsg = err.message;
              } else if (typeof err === 'object') {
                errMsg = JSON.stringify(err, null, 2);
              } else {
                errMsg = String(err);
              }
              this.emitAndLogError(errMsg, 'Failed to update leverage');
            }
          }),
        );
        orderIds.push(...results);
      }

      if (lot.length > 1) {
        try {
          const { data } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, {
            batchOrders: JSON.stringify(lot),
          });

          // lock fail bingx non error returned when submitting trigger orders...
          if (data.code !== 0 && !data.msg.includes('Msg: lock fail')) {
            errMsg = data.msg;
            this.emitAndLogError(errMsg);
          }

          data?.forEach?.((o: any) => {
            // lock fail bingx non error returned when submitting trigger orders...
            if (o.code && !o?.msg?.includes('Msg: lock fail')) {
              this.emitAndLogError(o.msg);
            } else {
              orderIds.push(o.data.order.id);
            }
          });
        } catch (err: any) {
          if (err?.response?.data?.msg) {
            errMsg = err.response.data.msg;
          } else if (err?.response?.msg) {
            errMsg = err.response.msg;
          } else if (err?.msg) {
            errMsg = err.msg;
          } else if (err?.message) {
            errMsg = err.message;
          } else if (typeof err === 'object') {
            errMsg = JSON.stringify(err, null, 2);
          } else {
            errMsg = String(err);
          }
          this.emitAndLogError(errMsg, 'Failed to switch position mode');
        }
      }
    }

    if (errMsg !== '') {
      throw new Error(errMsg);
    }

    return orderIds;
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 100 } = opts;

    if (!symbol) {
      // BingX requires symbol for execution queries
      return [];
    }

    const executions: Execution[] = [];

    try {
      const market = this.store.marketsBySymbol[symbol];
      const params: Record<string, any> = {
        symbol: market?.id || this.toBingxSymbol(symbol), // BingX uses BTC-USDT / BTC-USDC format
      };

      // Only add optional params if provided - BingX might not need them
      if (limit) {
        params.limit = Math.min(limit, 500);
      }
      if (orderId) {
        params.orderId = orderId;
      }
      if (from) {
        params.startTime = from;
      }
      if (to) {
        params.endTime = to;
      }

      this.log(`fetchHistoricalFills params: ${JSON.stringify(params)}`, LogSeverity.Debug);

      const { data } = await this.xhr.get(ENDPOINTS.USER_TRADES, { params });

      this.log(`fetchHistoricalFills response: ${JSON.stringify(data)}`, LogSeverity.Debug);

      if (data.code !== 0) {
        this.log(`Failed to fetch executions: ${data.msg}`, LogSeverity.Warning);
        return [];
      }

      // BingX returns fills in data.data.fill_history_orders (fillHistory) or data.data.fill_orders (allFillOrders)
      const responseData = data?.data;
      const fills = Array.isArray(responseData?.fill_history_orders)
        ? responseData.fill_history_orders
        : Array.isArray(responseData?.fill_orders)
          ? responseData.fill_orders
          : Array.isArray(responseData)
            ? responseData
            : [];

      this.log(`fetchHistoricalFills found ${fills.length} fills`, LogSeverity.Debug);

      for (const fill of fills) {
        // BingX uses filledTime in ISO format (e.g., "2026-01-13T17:13:17.000+08:00") or filledTm in ms
        let timestamp: number;
        const filledTime = fill.filledTime || fill.filledTm || fill.time;
        if (typeof filledTime === 'string' && filledTime.includes('T')) {
          // ISO format string
          timestamp = Math.floor(new Date(filledTime).getTime() / 1000);
        } else {
          // Milliseconds
          timestamp = Math.floor(parseInt(filledTime || Date.now()) / 1000);
        }
        executions.push({
          timestamp,
          symbol: fill.symbol?.replace(/-/g, '') || symbol, // Convert BTC-USDT to BTCUSDT
          orderId: fill.orderId || fill.order_id,
          price: parseFloat(fill.price || fill.filledPrice || '0'),
          qty: parseFloat(fill.volume || fill.qty || fill.filledQty || '0'),
          side: String(fill.side || '').toUpperCase() === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
          commission: Math.abs(parseFloat(fill.commission || fill.fee || '0')),
          commissionAsset: fill.commissionAsset || fill.currency || 'USDT',
        });
      }
    } catch (err: any) {
      const errMsg = err?.response?.data?.msg || err?.message || String(err);
      this.log(`Failed to fetch executions: ${errMsg}`, LogSeverity.Warning);
    }

    return executions;
  };

  private placeOrderBatch = async (payloads: any[]) => {
    const orderIds = [] as string[];
    let errMsg = '';

    for (const payload of payloads) {
      try {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, payload);
        if (data.code !== 0) {
          errMsg = this.normalizeOrderPlacementError(
            payload?.symbol?.replace(/-/g, '') || 'unknown-symbol',
            data.msg || '',
          );
          this.emitAndLogError(errMsg);
        } else {
          const acceptedOrder = this.mapOrder(data?.data?.order || {});
          if (acceptedOrder?.status === OrderStatus.Open && acceptedOrder.type !== OrderType.Market) {
            this.store.addOrUpdateOrder(acceptedOrder);
          }
          orderIds.push(data?.data?.order?.orderID || data?.data?.order?.orderId);
        }
      } catch (err: any) {
        if (err?.response?.data?.msg) {
          errMsg = err.response.data.msg;
        } else if (err?.response?.msg) {
          errMsg = err.response.msg;
        } else if (err?.msg) {
          errMsg = err.msg;
        } else if (err?.message) {
          errMsg = err.message;
        } else if (typeof err === 'object') {
          errMsg = JSON.stringify(err, null, 2);
        } else {
          errMsg = String(err);
        }
        errMsg = this.normalizeOrderPlacementError(payload?.symbol?.replace(/-/g, '') || 'unknown-symbol', errMsg);
        this.emitAndLogError(errMsg, 'Failed to switch margin type');
      }
    }

    if (errMsg !== '') {
      throw new Error(errMsg);
    }

    return orderIds;
  };
}
