import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import dayjs from 'dayjs';
import groupBy from 'lodash/groupBy';
import omit from 'lodash/omit';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';
import { forEachSeries } from 'p-iteration';

import { createApiWithSecondaryKey } from '../../../createApiWithSecondaryKey';
import {
  Candle,
  ExchangeName,
  ExchangeOptions,
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
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, subtract } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { markBinanceOrdersCanceled } from './binance.utils';
import { createFAPI, createPAPI, createPublicAPI } from './binancepm.api';
import {
  ENDPOINTS,
  GET_POSITION_RISK_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from '../../../exchanges/binance/binancepm.types';
import { BinancePMPrivateWebsocket } from './binancepm.ws-private';
import { BinancePMPublicWebsocket } from './binancepm.ws-public';

type BinancePMResponseRecord = Record<string, any>;

const describeBinancePMResponseShape = (payload: unknown): string => {
  if (Array.isArray(payload)) return `array(${payload.length})`;
  if (payload === null) return 'null';
  if (typeof payload === 'object') {
    const keys = Object.keys(payload as Record<string, unknown>).slice(0, 6);
    return keys.length ? `object(${keys.join(',')})` : 'object';
  }
  return typeof payload;
};

const getBinancePMResponseMessage = (payload: unknown): string => {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return '';
  const record = payload as Record<string, unknown>;
  const message = record.msg || record.message || record.error;
  const code = record.code;
  const parts = [code !== undefined ? `code=${String(code)}` : '', message ? String(message) : ''].filter(Boolean);
  return parts.join(' ');
};

const requireBinancePMArrayResponse = <T>(endpoint: string, payload: unknown): T[] => {
  if (Array.isArray(payload)) return payload as T[];

  const shape = describeBinancePMResponseShape(payload);
  const message = getBinancePMResponseMessage(payload);
  throw new Error(
    `Binance PM ${endpoint} response malformed: expected array, received ${shape}${message ? ` (${message})` : ''}`,
  );
};

// TODO: translate our err messages
export class BinancePMExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Binance PM', /^[.A-Z:/a-z0-9_-]{1,32}$/, {
    mint: () => 'x-s3vFFRQp' + uuid(),
  });

  papiXhr: Axios;
  fapiXhr: Axios;
  unlimitedXHR: Axios;
  publicXhr: Axios;

  publicWebsocket: BinancePMPublicWebsocket;
  privateWebsocket: BinancePMPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    // Route reads on the authed papi/fapi clients to the secondary (read-only)
    // key when one is set; writes stay on the primary. Keep papi's raw primary/
    // secondary clients for explicit per-key validation. Default-off: with no
    // secondary key all three clients are identical (see createApiWithSecondaryKey).
    const [papiXhr, papiPrimaryXhr, papiSecondaryXhr] = createApiWithSecondaryKey(opts, this, createPAPI);
    this.papiXhr = papiXhr;
    this._primaryXhr = papiPrimaryXhr;
    this._secondaryXhr = papiSecondaryXhr;
    // fapiXhr is currently unused by binancepm; leave it on the primary key
    // (no read-split wrapper) until a private FAPI read path exists.
    this.fapiXhr = createFAPI(opts, this);
    this.unlimitedXHR = createPAPI(opts, this);
    this.publicXhr = createPublicAPI(opts, this);

    this.publicWebsocket = new BinancePMPublicWebsocket(this);
    this.privateWebsocket = new BinancePMPrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  getAccount = async () => {
    const { data } = await this.papiXhr.get<unknown>(ENDPOINTS.BALANCE);
    const [{ accountAlias }] = requireBinancePMArrayResponse<BinancePMResponseRecord>(ENDPOINTS.BALANCE, data);

    return { userId: accountAlias };
  };

  validateAccount = async () => {
    try {
      await this._primaryXhr!.get(ENDPOINTS.ACCOUNT);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }

    if (this.options.secondary_key) {
      try {
        await this._secondaryXhr!.get(ENDPOINTS.ACCOUNT);
      } catch (err: any) {
        return { error: `Secondary Key: ${err?.response?.data?.msg || err?.message}` };
      }
    }
    // Key validated; balance is best-effort for the new-account gate.
    return {
      error: '',
      balance: await this.fetchBalance()
        .then((r) => r.balance)
        .catch(() => undefined),
    };
  };

  // The shared public exchange has no keys, so its markets carry leverage.max = 1.
  private loadAccountMaxLeverage = () =>
    this.loadAccountMarketOverlay(async () => {
      const { data } = await this.papiXhr.get<unknown>(ENDPOINTS.LEVERAGE_BRACKET);
      const brackets = requireBinancePMArrayResponse<Record<string, any>>(ENDPOINTS.LEVERAGE_BRACKET, data);
      const maxBySymbol = new Map(
        brackets
          .map((b): [string, number] => [b.symbol, Number(v(b.brackets?.[0], 'initialLeverage'))])
          .filter(([, max]) => max > 0),
      );
      return (markets) =>
        markets.map((m) => {
          const max = maxBySymbol.get(m.symbol);
          return max ? { ...m, limits: { ...m.limits, leverage: { ...m.limits.leverage, max } } } : m;
        });
    });

  start = async () => {
    if (this.needsAccountMarketOverlay) {
      await this.loadAccountMaxLeverage();
      if (this.isDisposed) return;
    }

    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      // load initial market data
      // then we can poll for live data
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      // load initial tickers data
      // then we use websocket for live data
      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Binance markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      // start websocket streams
      this.publicWebsocket.connectAndSubscribe();
    }

    if (!this.isPublicDataOnly) {
      await this._startPrivate();
    } else {
      await this._startPublicDataOnly();
    }
  };

  /** The last full order fetch failed; the tick refetches until one succeeds. */
  ordersSnapshotStale = false;

  private lastOrdersRetryAt = 0;

  // At most every 15s: each failure emits, and the tick runs far more often.
  private retryOrdersSnapshot = async () => {
    if (Date.now() - this.lastOrdersRetryAt < 15_000) return;
    this.lastOrdersRetryAt = Date.now();
    const orders = await this.fetchOrders();
    if (this.isDisposed || this.ordersSnapshotStale) return;
    this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
  };

  _startPrivate = async () => {
    this.privateWebsocket.connectAndSubscribe();

    // fetch current position mode (Hedge/One-way)
    this.store.setSetting('isHedged', await this.fetchPositionMode());

    // start ticking live data
    // balance, tickers, positions
    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Binance`);

    // fetch unfilled orders
    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    // A failed fetch returns the current (at startup, empty) store; marking that
    // loaded would show "no open orders". The tick retries until one succeeds.
    if (this.ordersSnapshotStale) return;

    this.log(`Loaded Binance orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });
  };

  _startPublicDataOnly = async () => {
    // start ticking live data
    // balance, tickers, positions
    if (this.isDisposed) return;

    this.log(`Ready to watch public data on Binance`);
    this.store.update({
      orders: [],
      balance: {
        used: 0,
        free: 0,
        total: 0,
        upnl: 0,
      },
      positions: [],
      loaded: { ...this.store.loaded, orders: true, balance: true, positions: true },
    });
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        if (this.ordersSnapshotStale) await this.retryOrdersSnapshot();
        if (this.isDisposed) return;
        if (count % 16 === 0) {
          const positionsWithRisk = await this.fetchPositions();
          this.store.update({ positions: positionsWithRisk });
          if (this.isDisposed) return;
        }
        const { balance } = await this.fetchBalance();
        if (this.isDisposed) return;

        this.store.update({
          balance,
          loaded: {
            ...this.store.loaded,
            balance: true,
            positions: true,
          },
        });
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), 5_000);
      }
    }
  };

  fetchMarkets = async () => {
    try {
      const { data: marketData } = await this.publicXhr.get<{ symbols?: unknown }>(ENDPOINTS.MARKETS);
      const symbols = requireBinancePMArrayResponse<BinancePMResponseRecord>(
        `${ENDPOINTS.MARKETS}.symbols`,
        marketData?.symbols,
      );

      const { data: leverageBracketData } = this.isPublicDataOnly
        ? { data: [] }
        : await this.papiXhr.get<unknown>(ENDPOINTS.LEVERAGE_BRACKET);
      const leverageBrackets = requireBinancePMArrayResponse<BinancePMResponseRecord>(
        ENDPOINTS.LEVERAGE_BRACKET,
        leverageBracketData,
      );

      const markets: Market[] = symbols.reduce<Market[]>((acc, m) => {
        if (
          !['PERPETUAL', 'TRADIFI_PERPETUAL'].includes(v(m, 'contractType')) ||
          !['USDT', 'USDC'].includes(v(m, 'marginAsset'))
        ) {
          return acc;
        }

        const filters = Array.isArray(m.filters) ? m.filters : [];
        const p = filters.find((f: any) => v(f, 'filterType') === 'PRICE_FILTER');
        const amt = filters.find((f: any) => v(f, 'filterType') === 'LOT_SIZE');
        const mAmt = filters.find((f: any) => v(f, 'filterType') === 'MARKET_LOT_SIZE');
        if (!p || !amt || !mAmt) return acc;

        const brackets = leverageBrackets.find((b) => b.symbol === m.symbol)?.brackets;
        const baseAsset = v(m, 'baseAsset');
        const quoteAsset = v(m, 'quoteAsset');
        const marginAsset = v(m, 'marginAsset');

        acc.push({
          id: `${baseAsset}/${quoteAsset}:${marginAsset}`,
          symbol: m.symbol,
          base: baseAsset,
          quote: quoteAsset,
          active: m.status === 'TRADING',
          precision: {
            amount: parseFloat(v(amt, 'stepSize')),
            price: parseFloat(v(p, 'tickSize')),
          },
          limits: {
            amount: {
              min: Math.max(parseFloat(v(amt, 'minQty')), parseFloat(v(mAmt, 'minQty'))),
              max: Math.min(parseFloat(v(amt, 'maxQty')), parseFloat(v(mAmt, 'maxQty'))),
            },
            leverage: {
              min: 1,
              max: v(brackets?.[0], 'initialLeverage') || 1,
            },
          },
        });
        return acc;
      }, []);

      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    try {
      const { data: dailyData } = await this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_24H);
      const dailys = requireBinancePMArrayResponse<BinancePMResponseRecord>(ENDPOINTS.TICKERS_24H, dailyData);

      const { data: bookData } = await this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_BOOK);
      const books = requireBinancePMArrayResponse<BinancePMResponseRecord>(ENDPOINTS.TICKERS_BOOK, bookData);

      const { data: priceData } = await this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_PRICE);
      const prices = requireBinancePMArrayResponse<BinancePMResponseRecord>(ENDPOINTS.TICKERS_PRICE, priceData);

      return books.reduce((acc: Ticker[], book) => {
        const market = this.store.marketsBySymbol[book.symbol];

        const daily = dailys.find((d) => d.symbol === book.symbol)!;
        const price = prices.find((p) => p.symbol === book.symbol)!;

        if (!market || !daily || !price) return acc;

        const ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: parseFloat(v(book, 'bidPrice')),
          ask: parseFloat(v(book, 'askPrice')),
          last: parseFloat(v(daily, 'lastPrice')),
          mark: parseFloat(v(price, 'markPrice')),
          index: parseFloat(v(price, 'indexPrice')),
          percentage: parseFloat(v(daily, 'priceChangePercent')),
          fundingRate: parseFloat(v(price, 'lastFundingRate')),
          fundingTime: parseFloat(v(price, 'nextFundingTime')),
          volume: parseFloat(daily.volume),
          quoteVolume: parseFloat(v(daily, 'quoteVolume')),
          openInterest: 0, // Binance doesn't provides all tickers data
        };

        return [...acc, ticker];
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  fetchPositions = async () => {
    // const { positions } = await this.fetchPositionsAndBalance();
    // return positions;
    try {
      const { data } = await this.papiXhr.get<unknown>(ENDPOINTS.POSITION_RISK);
      const rows = requireBinancePMArrayResponse<GET_POSITION_RISK_RESPONSE[number]>(ENDPOINTS.POSITION_RISK, data);
      const formattedPositions = rows.map((p) => {
        // const foundPosition = this.store.positions.find((pos) => pos.symbol === p.symbol);
        const pSide = p.positionSide;
        const contracts = parseFloat(p.positionAmt);
        const side =
          (pSide in POSITION_SIDE && POSITION_SIDE[pSide]) || (contracts > 0 ? PositionSide.Long : PositionSide.Short);
        const positionMode = p.positionSide === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        // const positionMode = p.positionSide === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        // const marginType = p.marginType === 'isolated' ? MarginType.Isolated : MarginType.Cross;
        const notional = Math.abs(parseFloat(p.notional));
        const leverage = parseFloat(p.leverage);
        const res: Position = {
          id: createPositionId(p.symbol, side, positionMode),
          symbol: p.symbol,
          side,
          entryPrice: parseFloat(p.entryPrice),
          notional,
          leverage,
          unrealizedPnl: parseFloat(p.unRealizedProfit),
          contracts: Math.abs(contracts),
          liquidationPrice: parseFloat(p.liquidationPrice),
          // PM is cross-only; isolatedMargin is not provided. Compute from notional/leverage.
          marginSize: leverage > 0 ? notional / leverage : undefined,
          marginType: MarginType.Cross,
          positionMode,
          info: p,
        };
        return res;
      });

      const createFakePosition = (m: Market, side: PositionSide, positionMode: PositionMode) => {
        return {
          id: createPositionId(m.symbol, side, positionMode),
          symbol: m.symbol,
          side,
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          leverage: 10,
          liquidationPrice: 0,
          positionMode,
          marginType: MarginType.Cross,
        };
      };

      const mode = this.store.options.isHedged ? PositionMode.Hedged : PositionMode.OneWay;
      const fakePositions = this.store.markets.reduce((acc: Position[], m) => {
        acc.push(...[createFakePosition(m, PositionSide.Long, mode), createFakePosition(m, PositionSide.Short, mode)]);
        return acc;
      }, []);

      return uniqBy([...formattedPositions, ...fakePositions], 'id');
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.positions;
    }
  };

  fetchBalance = async () => {
    try {
      const { data } = await this.papiXhr.get<unknown>(ENDPOINTS.BALANCE);
      // {
      //   "uniMMR": "99999999",
      //   "accountEquity": "62.4579898",
      //   "actualEquity": "62.46423623",
      //   "accountInitialMargin": "0.0",
      //   "accountMaintMargin": "0.0",
      //   "accountStatus": "NORMAL",
      //   "virtualMaxWithdrawAmount": "62.4579898",
      //   "totalAvailableBalance": "62.4579898",
      //   "totalMarginOpenLoss": "0.0",
      //   "updateTime": 1745161574194
      // }

      // {
      //   "uniMMR": "92.32876265",
      //   "accountEquity": "62.36925527",
      //   "actualEquity": "62.37549282",
      //   "accountInitialMargin": "1.68878185",
      //   "accountMaintMargin": "0.67551274",
      //   "accountStatus": "NORMAL",
      //   "virtualMaxWithdrawAmount": "60.67283414",
      //   "totalAvailableBalance": "60.67283414",
      //   "totalMarginOpenLoss": "0.0",
      //   "updateTime": 1745167476298
      // }

      // {
      //   "asset": "USDT",
      //   "totalWalletBalance": "62.42036929",
      //   "crossMarginAsset": "0.0",
      //   "crossMarginBorrowed": "0.0",
      //   "crossMarginFree": "0.0",
      //   "crossMarginInterest": "0.0",
      //   "crossMarginLocked": "0.0",
      //   "umWalletBalance": "62.42036929",
      //   "umUnrealizedPNL": "-0.00906277",
      //   "cmWalletBalance": "0.0",
      //   "cmUnrealizedPNL": "0.0",
      //   "updateTime": 1745167450111,
      //   "negativeBalance": "0.0"
      // },

      let total = 0;
      let used = 0;
      let free = 0;
      let upnl = 0;

      const rows = requireBinancePMArrayResponse<BinancePMResponseRecord>(ENDPOINTS.BALANCE, data);
      for (let i = 0; i < rows.length; i++) {
        if (rows[i].asset === 'USDT' || rows[i].asset === 'USDC' || rows[i].asset === 'FDUSD') {
          const item = rows[i];
          const itemTotal = parseFloat(item.totalWalletBalance);
          const itemFree = parseFloat(item.crossMarginFree);
          const itemUsed = itemTotal - itemFree;
          free += itemFree;
          total += itemTotal;
          used += itemUsed;
          upnl += parseFloat(item.umUnrealizedPNL);
        }
      }

      // const total = parseFloat(data.totalWalletBalance);
      // const free = parseFloat(data.availableBalance);
      // const upnl = parseFloat(data.totalUnrealizedProfit);
      // const used = parseFloat(data.totalInitialMargin);

      const balance = { total, free, used, upnl, info: data };

      return { balance };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { balance: this.store.balance };
    }
  };

  fetchRegularOrders = async () => {
    const { data } = await this.papiXhr.get<unknown>(ENDPOINTS.OPEN_ORDERS);
    const rows = requireBinancePMArrayResponse<BinancePMResponseRecord>(ENDPOINTS.OPEN_ORDERS, data);

    const orders: Order[] = rows.map((o) => {
      return {
        id: v(o, 'clientOrderId'),
        status: OrderStatus.Open,
        symbol: o.symbol,
        type: ORDER_TYPE[o.type],
        side: ORDER_SIDE[o.side],
        price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')),
        amount: parseFloat(v(o, 'origQty')),
        reduceOnly: v(o, 'reduceOnly') || false,
        filled: parseFloat(v(o, 'executedQty')),
        lastUpdate: virtualClock.getCurrentTimeMs(),
        remaining: subtract(v(o, 'origQty'), v(o, 'executedQty')),
        info: o,
        ...(ORDER_TYPE[o.type] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
        ...(ORDER_TYPE[o.type] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
      };
    });

    return orders;
  };

  // [
  //   {
  //       "newClientStrategyId": "Dm34U0aNA2QL7ydi2Z6OlI",
  //       "strategyId": 60684715,
  //       "strategyStatus": "NEW",
  //       "strategyType": "TAKE_PROFIT_MARKET",
  //       "origQty": "0.083",
  //       "price": "0",
  //       "reduceOnly": false,
  //       "side": "SELL",
  //       "positionSide": "BOTH",
  //       "stopPrice": "140000",
  //       "symbol": "BTCUSDT",
  //       "bookTime": 1745256761235,
  //       "updateTime": 1745256761235,
  //       "timeInForce": "GTC",
  //       "workingType": "MARK_PRICE",
  //       "priceProtect": false,
  //       "goodTillDate": 0,
  //       "selfTradePreventionMode": "EXPIRE_MAKER"
  //   }
  // ]
  fetchConditionalOrders = async () => {
    const { data } = await this.papiXhr.get<unknown>(ENDPOINTS.OPEN_CONDITIONAL_ORDERS);
    const rows = requireBinancePMArrayResponse<BinancePMResponseRecord>(ENDPOINTS.OPEN_CONDITIONAL_ORDERS, data);

    const orders: Order[] = rows.map((o) => {
      return {
        id: v(o, 'newClientStrategyId'),
        status: OrderStatus.Open,
        symbol: o.symbol,
        type: ORDER_TYPE[o.strategyType],
        side: ORDER_SIDE[o.side],
        price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')),
        amount: parseFloat(v(o, 'origQty')),
        reduceOnly: v(o, 'reduceOnly') || false,
        filled: parseFloat(v(o, 'executedQty') || 0),
        lastUpdate: virtualClock.getCurrentTimeMs(),
        remaining: subtract(v(o, 'origQty'), v(o, 'executedQty') || 0),
        info: o,
        ...(ORDER_TYPE[o.type] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
        ...(ORDER_TYPE[o.type] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
      };
    });

    return orders;
  };

  fetchOrders = async () => {
    try {
      const orders = [...(await this.fetchRegularOrders()), ...(await this.fetchConditionalOrders())];
      this.ordersSnapshotStale = false;
      return orders;
    } catch (err: any) {
      // The tick retries a stale snapshot every 15s; toast only the first failure.
      const message = err?.response?.data?.msg || err?.message;
      if (this.ordersSnapshotStale) this.log(`fetchOrders error: ${message}`, LogSeverity.Warning);
      else this.emitAndLogError(message);
      this.ordersSnapshotStale = true;
      return this.store.orders;
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const limit = Math.min(opts.limit || 500, 1500);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.publicXhr.get<unknown>(ENDPOINTS.KLINE, {
      params: {
        symbol: opts.symbol,
        interval,
        startTime: start.valueOf(),
        endTime: end.valueOf(),
        ...(opts.limit
          ? {
              limit,
            }
          : {}),
      },
    });

    const rows = requireBinancePMArrayResponse<any[]>(ENDPOINTS.KLINE, data);
    const candles: Candle[] = rows.map(([time, open, high, low, close, volume]) => {
      return {
        timestamp: time / 1000,
        open: parseFloat(open),
        high: parseFloat(high),
        low: parseFloat(low),
        close: parseFloat(close),
        volume: parseFloat(volume),
      };
    });

    return candles;
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchPositionMode = async () => {
    const { data } = await this.papiXhr.get(ENDPOINTS.HEDGE_MODE);
    return data.dualSidePosition === true;
  };

  changePositionMode = async (hedged: boolean) => {
    if (this.store.positions.filter((p) => p.contracts > 0).length > 0) {
      this.emitAndLogError('Please close all positions before switching position mode');
      return;
    }

    try {
      await this.papiXhr.post(ENDPOINTS.HEDGE_MODE, {
        dualSidePosition: hedged ? 'true' : 'false',
      });
      this.store.setSetting('isHedged', hedged);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  changeMarginType = async (_isolated: boolean, _symbol?: string) => {
    this.emitAndLogError('Portfolio margin is not supported');
    throw new Error('Portfolio margin is not supported');
  };

  setLeverage = async (symbol: string, inputLeverage: number) => {
    const market = this.store.marketsBySymbol[symbol];
    const position = this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        await this.papiXhr.post(ENDPOINTS.SET_LEVERAGE, {
          symbol,
          leverage,
        });

        this.store.updatePositions([
          [{ id: createPositionId(symbol, PositionSide.Long) }, { leverage }],
          [{ id: createPositionId(symbol, PositionSide.Short) }, { leverage }],
        ]);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
      }
    }
  };

  cancelOrders = async (orders: Order[]) => {
    try {
      // Define conditional order types
      const conditionalTypes = [OrderType.StopLoss, OrderType.TakeProfit, OrderType.TrailingStopLoss];

      const groupedBySymbol = groupBy(orders, 'symbol');
      const requests = Object.entries(groupedBySymbol).map(([symbol, symbolOrders]) => ({
        symbol,
        orders: symbolOrders,
      }));

      await forEachSeries(requests, async (request) => {
        // Cancel orders one by one
        await forEachSeries(request.orders, async (order) => {
          try {
            if (conditionalTypes.includes(order.type)) {
              // Use conditional order endpoint for SL/TP/TRAILING_STOP orders
              await this.papiXhr.delete(ENDPOINTS.CONDITIONAL_ORDER, {
                params: {
                  symbol: request.symbol,
                  newClientStrategyId: order.id,
                },
              });
            } else {
              // Use regular order endpoint for other order types
              await this.papiXhr.delete(ENDPOINTS.ORDER, {
                params: {
                  symbol: request.symbol,
                  origClientOrderId: order.id,
                },
              });
            }
            markBinanceOrdersCanceled(this.store, [order]);
          } catch (err: any) {
            this.emitAndLogError(`Failed to cancel order ${order.id}: ${err?.response?.data?.msg || err?.message}`);
            throw err;
          }
        });
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      const symbolOrders = this.store.orders.filter((order) => order.symbol === symbol);
      await this.papiXhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
        params: { symbol },
      });

      markBinanceOrdersCanceled(this.store, symbolOrders);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
    };

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    await this.cancelOrders([order]);
    return await this.placeOrder(newOrder);
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    const payloads = this.formatCreateOrder(opts);
    return await this.placeOrderBatch(payloads);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    const requests = orders.flatMap((o) => this.formatCreateOrder(o));
    return await this.placeOrderBatch(requests);
  };

  private formatCreateOrder = (opts: PlaceOrderOpts) => {
    const clientOrderId = this.resolveCallerClientOrderId(opts);
    if (opts.type === OrderType.TrailingStopLoss) {
      return this.formatCreateTrailingStopLossOrder(opts);
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

    const pSide = this.getOrderPositionSide(opts);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);

    // We use price only for limit orders
    // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    // Binance stopPrice only for SL or TP orders
    const priceField = isStopOrTP ? 'stopPrice' : 'price';

    const reduceOnly = !this.store.options.isHedged && opts.reduceOnly;
    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    const req = omitUndefined({
      symbol: opts.symbol,
      positionSide: pSide,
      side: inverseObj(ORDER_SIDE)[opts.side],
      type: inverseObj(ORDER_TYPE)[opts.type],
      quantity: amount ? `${amount}` : undefined,
      [priceField]: price ? `${price}` : undefined,
      timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
      closePosition: isStopOrTP && !amount ? 'true' : undefined,
      reduceOnly: reduceOnly && (!isStopOrTP || amount) ? 'true' : undefined,
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

    const payloads: Array<Record<string, any>> = times(lots, () => ({
      ...req,
      quantity: `${lotSize}`,
    }));

    if (rest) {
      payloads.push({ ...req, quantity: `${rest}` });
    }

    if (opts.stopLoss) {
      payloads.push({
        ...omit(req, 'price'),
        side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
        type: inverseObj(ORDER_TYPE)[OrderType.StopLoss],
        stopPrice: `${adjust(opts.stopLoss, pPrice)}`,
        timeInForce: 'GTC',
        closePosition: 'true',
        workingType: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'CONTRACT_PRICE' : 'MARK_PRICE',
      });
    }

    if (opts.takeProfit) {
      payloads.push({
        ...omit(req, 'price'),
        side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
        type: inverseObj(ORDER_TYPE)[OrderType.TakeProfit],
        stopPrice: `${adjust(opts.takeProfit, pPrice)}`,
        timeInForce: 'GTC',
        closePosition: 'true',
        workingType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'CONTRACT_PRICE',
      });
    }

    // A unique ID per payload, otherwise Binance duplicates the IDs in batches.
    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
    for (const payload of payloads) {
      payload.newClientOrderId = clientOrderId ?? 'x-s3vFFRQp' + uuid();
    }

    return payloads;
  };

  private formatCreateTrailingStopLossOrder = (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickersBySymbol[opts.symbol];

    const pSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;

    const position = this.store.positions.find((p) => p.symbol === opts.symbol && p.side === pSide);

    if (!market) throw new Error(`Market ${opts.symbol} not found`);
    if (!ticker) throw new Error(`Ticker ${opts.symbol} not found`);

    if (!position) {
      throw new Error(`Position ${opts.symbol} and side ${pSide} not found`);
    }

    const priceDistance = adjust(
      Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
      market.precision.price,
    );

    const distancePercentage = Math.round(((priceDistance * 100) / ticker.last) * 10) / 10;

    const payload = {
      symbol: opts.symbol,
      positionSide: this.getOrderPositionSide(opts),
      side: inverseObj(ORDER_SIDE)[opts.side],
      type: inverseObj(ORDER_TYPE)[OrderType.TrailingStopLoss],
      quantity: `${position.contracts}`,
      callbackRate: `${distancePercentage}`,
      priceProtect: 'true',
      newClientOrderId: uuid(),
    };

    return [payload];
  };

  private getOrderPositionSide = (opts: PlaceOrderOpts) => {
    let positionSide = 'BOTH';

    // We need to specify side of the position to interract with
    // if we are in hedged mode on the binance account
    if (this.store.options.isHedged) {
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

  private placeOrderBatch = async (payloads: any[]) => {
    const orderIds = [] as string[];

    // Split payloads into groups based on order type
    const regularOrders: any[] = [];
    const conditionalOrders: any[] = []; // For specific types like STOP_MARKET, TAKE_PROFIT_MARKET, TRAILING_STOP_MARKET

    // Define types to split
    const conditionalTypes = ['STOP_MARKET', 'TAKE_PROFIT_MARKET', 'TRAILING_STOP_MARKET'];

    // Separate payloads into groups
    for (const payload of payloads) {
      if (conditionalTypes.includes(payload.type)) {
        const { type, quantity, ...rest } = payload;
        conditionalOrders.push({
          strategyType: type,
          ...(+quantity
            ? { quantity }
            : {
                quantity:
                  this.store.positions.find(
                    (p) =>
                      p.symbol === payload.symbol &&
                      ((payload.side === 'SELL' && p.side === PositionSide.Long) ||
                        (payload.side === 'BUY' && p.side === PositionSide.Short)),
                  )?.contracts || 0,
              }),
          ...rest,
        });
      } else {
        regularOrders.push(payload);
      }
    }

    // Process regular orders first
    for (const payload of regularOrders) {
      try {
        await this.unlimitedXHR.post(ENDPOINTS.ORDER, payload);
        orderIds.push(payload.newClientOrderId);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    // Process conditional orders after
    for (const payload of conditionalOrders) {
      try {
        await this.unlimitedXHR.post(ENDPOINTS.CONDITIONAL_ORDER, payload);
        orderIds.push(payload.newClientOrderId);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    return orderIds;
  };
}
