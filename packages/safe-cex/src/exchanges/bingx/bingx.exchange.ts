import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';

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
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust, subtract } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
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
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from './bingx.types';
import { BingxPrivateWebsocket } from './bingx.ws-private';
import { BingxPublicWebsocket } from './bingx.ws-public';

export class BingxExchange extends BaseExchange {
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

  private normalizeOrderPlacementError = (symbol: string, errorMessage: string) => {
    if (/non-crypto symbol with one-way mode not support openapi and copy trade/i.test(errorMessage)) {
      return this.buildTradfiOneWayModeError(symbol, errorMessage);
    }

    return errorMessage;
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

      const ticker = {
        id: market.id,
        symbol: market.symbol,
        bid: parseFloat(v(daily, 'bidPrice')),
        ask: parseFloat(v(daily, 'askPrice')),
        last: parseFloat(v(daily, 'lastPrice')),
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

    const orders: Order[] = data.data.orders.map((o) => {
      const order = {
        id: v(o, 'orderId'),
        status: OrderStatus.Open,
        symbol: o.symbol.replace(/-/, ''),
        type: ORDER_TYPE[o.type],
        side: ORDER_SIDE[o.side],
        price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')),
        amount: parseFloat(v(o, 'origQty')),
        reduceOnly: v(o, 'reduceOnly') || false,
        close: v(o, 'closePosition') === true || v(o, 'closePosition') === 'true',
        timeInForce: v(o, 'postOnly') ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
        filled: parseFloat(v(o, 'executedQty')),
        lastUpdate: virtualClock.getCurrentTimeMs(),
        remaining: subtract(v(o, 'origQty'), v(o, 'executedQty')),
        info: o,
        ...(ORDER_TYPE[o.type] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
        ...(ORDER_TYPE[o.type] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
      };
      if (order.type === OrderType.StopLoss) {
        const ticker = this.store.tickersBySymbol[order.symbol];
        if (ticker) {
          if (
            (order.side === OrderSide.Buy && order.price < ticker.mark) ||
            (order.side === OrderSide.Sell && order.price > ticker.mark)
          ) {
            order.type = OrderType.TakeProfit;
            order.tpTriggerType = order.slTriggerType;
          }
        }
      }
      return order;
    });

    return orders;
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
        const {
          data: { data },
        } = await this.xhr.get<{ data: any[][] }>(ENDPOINTS.KLINE, {
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
    const isStopOrTP = order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit;
    const isFullPositionStop = isStopOrTP && (order.close || !order.amount || order.amount < 0.0001);

    const newOrder: Writable<PlaceOrderOpts> = {
      ...order,
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: isFullPositionStop ? 0 : order.amount,
      ...(isFullPositionStop ? { close: true } : {}),
      ...(order.reduceOnly ? { reduceOnly: true } : {}),
    };

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) {
      newOrder.amount = update.amount;
      newOrder.close = !update.amount;
    }

    const payloads = await this.formatCreateOrder(newOrder);
    if (payloads.length !== 1) {
      const message = 'BingX atomic order replacement requires exactly one replacement payload';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    try {
      const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_REPLACE, {
        ...payloads[0],
        cancelOrderId: order.id,
        cancelReplaceMode: 'STOP_ON_FAILURE',
      });
      if (data.code !== 0 || data?.data?.cancelResult !== 'SUCCESS' || data?.data?.newOrderResult !== 'SUCCESS') {
        throw new Error(data.msg || 'BingX atomic cancel-and-replace failed');
      }

      return [String(data.data.newOrderId)];
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
    // prefetch position mode to prevent dispatching request for every order
    await this.getOrFetchPositionMode();
    const payloads = (await Promise.all(orders.flatMap(this.formatCreateOrder.bind(this)))).flat();
    return await this.placeOrderBatch(payloads);
  };

  private formatCreateOrder = async (opts: PlaceOrderOpts) => {
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

    const payloads: Array<Record<string, any>> = wantsFullPositionStop
      ? [req]
      : times(lots, () => ({
          ...req,
          quantity: lotSize,
        }));

    if (rest) {
      payloads.push({ ...req, quantity: rest });
    }

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
      //clientOrderID: uuid(),
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

  fetchExecutions = async (opts: ExecutionsOptions): Promise<Execution[]> => {
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

      this.log(`fetchExecutions params: ${JSON.stringify(params)}`, LogSeverity.Debug);

      const { data } = await this.xhr.get(ENDPOINTS.USER_TRADES, { params });

      this.log(`fetchExecutions response: ${JSON.stringify(data)}`, LogSeverity.Debug);

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

      this.log(`fetchExecutions found ${fills.length} fills`, LogSeverity.Debug);

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
