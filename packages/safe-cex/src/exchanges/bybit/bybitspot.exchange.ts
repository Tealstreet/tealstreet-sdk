import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';
import type { ExchangeName } from '../../types';
import type { BybitPlaceOrderOpts } from './bybit.order-params';

import axios from 'axios';
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
  HistoricalTradeOptions,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { mergeUpdatesUsingLastUpdate } from '../../utils/mergeUpdatesUsingLastUpdate';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust, subtract } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createAPI } from './bybit.api';
import { getBybitOrderExecutionParams } from './bybit.order-params';
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
  POSITION_SIDE,
} from './bybit.types';
import { BybitPrivateWebsocket } from './bybitspot.ws-private';
import { BybitPublicWebsocket } from './bybitspot.ws-public';

const INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1_000;

export class BybitSpotExchange extends BaseExchange {
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

  positionMode: PositionMode | null = null;

  lastWebsocketPositionsUpdate: number | undefined;

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
    const marginAccountInfos = publicDataOnly ? null : this.fetchMarginAccountInfos();

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
    if (marginAccountInfos) await marginAccountInfos.catch(() => undefined);

    this.log(`Ready to trade on Bybit`);

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

      const fetchedPositions = await this.fetchPositions();
      if (this.isDisposed) return;
      const positions =
        this.lastWebsocketPositionsUpdate !== undefined &&
        virtualClock.getCurrentTimeMs() - this.lastWebsocketPositionsUpdate <
          INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY
          ? mergeUpdatesUsingLastUpdate(fetchedPositions, this.store.positions)
          : fetchedPositions;

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
      this.emitAndLogError(data.retMsg);
      return;
    }

    this.unifiedMarginStatus = data?.result?.unifiedMarginStatus;
  };

  tick = async () => {
    if (!this.isDisposed) {
      if (this.needsReinit) {
        await this.fetchOrdersPositionsAndBalance();
        this.needsReinit = false;
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
    const upnl = this.store.positions.reduce((acc, p) => acc + p.unrealizedPnl, 0);
    const total = subtract(parseFloat(firstAccount.totalEquity), upnl);

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
      const { data } = await this.xhr.get(ENDPOINTS.UNFILLED_ORDERS, {
        params: {
          category: 'spot',
          limit: 50,
          cursor,
        },
      });

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

    return orders;
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

      return { ...payload, qty: `${lotSize}` };
    });

    if (rest) payloads.push({ ...req, qty: `${rest}` });

    const responses = await mapSeries(payloads, async (p) => {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.CREATE_ORDER, p);
        return data;
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.retMsg || err.message);
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

    return fullfilled.map((resp) => resp.result.orderId);
  };

  placeOrders = async (orders: BybitPlaceOrderOpts[]): Promise<string[]> => {
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
      return this.placeTrailingStopLoss({ ...order, ...update });
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
        return this.placeStopLossOrTakeProfit({ ...order, ...update, type: projectedType });
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
        this.emitAndLogError(data.retMsg);
        throw new Error(data.retMsg);
      }
      const foundErr = data.retExtInfo?.list.find((l: any) => l.code !== 0);
      if (foundErr) {
        this.emitAndLogError(foundErr.msg);
        throw new Error(foundErr.msg);
      }
    });
  };

  cancelSymbolOrders = async (symbol: string, cancelTpSl = true) => {
    const tpOrSLOrders = cancelTpSl
      ? []
      : this.store.orders.filter((o) => o.symbol === symbol && o.type !== OrderType.Limit);

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
    }
  };

  mapPosition(p: Record<string, any>) {
    const entryPrice = parseFloat(v(p, 'avgPrice') || v(p, 'entryPrice') || 0);
    let contracts = parseFloat(p.size ?? 0);
    const side =
      p.positionIdx === 1
        ? PositionSide.Long
        : p.positionIdx === 2
          ? PositionSide.Short
          : POSITION_SIDE[p.side] || PositionSide.Long;

    let unrealizedPnl = parseFloat(v(p, 'unrealisedPnl') || 0);
    const market = this.store.marketsBySymbol[p.symbol];
    const contractsNative = contracts;
    let notional = parseFloat(v(p, 'positionValue') || 0);
    if (market?.isInverse) {
      const ticker = this.store.tickersBySymbol[p.symbol];
      const referencePrice = ticker?.last || ticker?.mark || entryPrice;
      if (referencePrice) {
        contracts = adjust(contracts / referencePrice, 0.0000001);
      }
      notional = contracts;
    } else if (!p.symbol.endsWith('USDT')) {
      const ticker = this.store.tickersBySymbol[p.symbol];
      if (market && market.quote === 'USDC' && ticker) {
        const direction = side === PositionSide.Long ? 1 : -1;
        unrealizedPnl = direction * (ticker.mark * contracts - entryPrice * contracts);
      }
    }

    const positionMode =
      p.positionIdx === undefined ? undefined : p.positionIdx === 0 ? PositionMode.OneWay : PositionMode.Hedged;
    const marginType =
      p.tradeMode === undefined ? undefined : p.tradeMode === 1 ? MarginType.Isolated : MarginType.Cross;
    const position: Position = {
      id: createPositionId(p.symbol, side, positionMode),
      symbol: p.symbol,
      side,
      entryPrice,
      notional,
      leverage: parseFloat(p.leverage),
      unrealizedPnl,
      realizedPnl: parseFloat(v(p, 'curRealisedPnl') || 0),
      contracts,
      liquidationPrice: parseFloat(v(p, 'liqPrice') || 0),
      positionMode,
      marginType,
      extra: {
        contractsNative,
      },
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    return position;
  }

  mapOrder(o: Record<string, any>) {
    const isStop = o.stopOrderType !== 'UNKNOWN' && o.stopOrderType !== '';

    const oPrice = isStop ? v(o, 'triggerPrice') : o.price;
    const oType = isStop ? v(o, 'stopOrderType') : v(o, 'orderType');

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
        amount: (oType === 'StopLoss' || oType === 'TakeProfit') && o.tpslMode === 'Full' ? 0 : parseFloat(o.qty ?? 0),
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
