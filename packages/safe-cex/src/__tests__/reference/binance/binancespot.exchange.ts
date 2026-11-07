import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import groupBy from 'lodash/groupBy';
import omit from 'lodash/omit';
import times from 'lodash/times';
import { forEachSeries } from 'p-iteration';

import {
  Candle,
  ExchangeName,
  ExchangeOptions,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Ticker,
  UpdateOrderOpts,
} from '../../../types';
import { v } from '../../../utils/get-key';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, subtract } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './binancespot.api';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TYPE_SPOT,
  TIME_IN_FORCE,
} from '../../../exchanges/binance/binancespot.types';
import { BinancePrivateWebsocket } from './binancespot.ws-private';
import { BinancePublicWebsocket } from './binancespot.ws-public';

// TODO: translate our err messages
export class BinanceSpotExchange extends BaseExchange {
  // Spot's legal range differs from USD-M/PM: no '.', ':' or '/'.
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Binance Spot', /^[A-Za-z0-9_-]{1,36}$/, {
    mint: () => 'x-s3vFFRQp' + uuid(),
  });

  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BinancePublicWebsocket;
  privateWebsocket: BinancePrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BinancePublicWebsocket(this);
    this.privateWebsocket = new BinancePrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.ACCOUNT);
      return {
        error: '',
        balance: await this.fetchPositionsAndBalance()
          .then((r) => r.balance)
          .catch(() => undefined),
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
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

  _startPrivate = async () => {
    this.privateWebsocket.connectAndSubscribe();

    // start ticking live data
    // balance, tickers, positions
    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Binance`);

    // fetch unfilled orders
    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

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

  tick = async () => {
    if (!this.isDisposed) {
      try {
        const { balance, positions } = await this.fetchPositionsAndBalance();
        if (this.isDisposed) return;

        this.store.update({
          balance,
          positions,
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
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };

  fetchMarkets = async () => {
    try {
      const {
        data: { symbols },
      } = await this.xhr.get<{ symbols: Array<Record<string, any>> }>(ENDPOINTS.MARKETS);

      const markets: Market[] = symbols.map((m) => {
        const baseAsset = v(m, 'baseAsset');
        const quoteAsset = v(m, 'quoteAsset');
        const lotSize = m.filters?.find((filter: Record<string, any>) => filter.filterType === 'LOT_SIZE');
        const priceFilter = m.filters?.find((filter: Record<string, any>) => filter.filterType === 'PRICE_FILTER');
        const amountStep = parseFloat(lotSize?.stepSize);
        const priceStep = parseFloat(priceFilter?.tickSize);

        return {
          id: `${baseAsset}/${quoteAsset}`,
          symbol: m.symbol,
          base: baseAsset,
          quote: quoteAsset,
          active: m.status === 'TRADING',
          isSpot: true,
          precision: {
            amount: amountStep > 0 ? amountStep : 10 ** -parseFloat(v(m, 'baseAssetPrecision')),
            price: priceStep > 0 ? priceStep : 10 ** -parseFloat(v(m, 'quoteAssetPrecision')),
          },
          limits: {
            amount: {
              min: parseFloat(lotSize?.minQty) || 0,
              max: parseFloat(lotSize?.maxQty) || 999_999_999,
            },
            leverage: {
              min: 1,
              max: 1,
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

  fetchTickers = async () => {
    try {
      const { data: dailys } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.TICKERS_24H);

      return dailys.reduce((acc: Ticker[], daily) => {
        const market = this.store.marketsBySymbol[daily.symbol];

        if (!market || !daily) return acc;

        const ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: parseFloat(v(daily, 'bidPrice')),
          ask: parseFloat(v(daily, 'askPrice')),
          last: parseFloat(v(daily, 'lastPrice')),
          mark: parseFloat(v(daily, 'lastPrice')),
          index: parseFloat(v(daily, 'lastPrice')),
          percentage: parseFloat(v(daily, 'priceChangePercent')),
          fundingTime: 0,
          fundingRate: 0,
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
    const { positions } = await this.fetchPositionsAndBalance();
    return positions;
  };

  fetchPositionsAndBalance = async () => {
    try {
      const { data } = await this.xhr.get<{ balances?: Array<Record<string, any>> }>(ENDPOINTS.ACCOUNT);
      const currencies: Record<string, any> = {};
      const coins: Record<string, any> = {};
      let free = 0;
      let used = 0;
      let total = 0;

      for (const assetBalance of data.balances || []) {
        const asset = assetBalance.asset;
        const assetFree = parseFloat(assetBalance.free || '0');
        const assetUsed = parseFloat(assetBalance.locked || '0');
        const assetTotal = assetFree + assetUsed;
        const price = this.getAssetUsdPrice(asset);

        currencies[asset] = {
          free: assetFree,
          used: assetUsed,
          total: assetTotal,
          upnl: 0,
          info: assetBalance,
        };
        coins[asset] = { base: asset, amount: assetTotal, notional: assetTotal * price };
        free += assetFree * price;
        used += assetUsed * price;
        total += assetTotal * price;
      }

      return {
        balance: { total, free, used, upnl: 0, currencies, coins, info: data },
        positions: [],
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { balance: this.store.balance, positions: this.store.positions };
    }
  };

  getAssetUsdPrice = (asset: string): number => {
    if (asset === 'USD' || asset === 'USDT' || asset === 'USDC') return 1;

    return (
      this.store.tickersBySymbol[`${asset}USDT`]?.last ||
      this.store.tickersBySymbol[`${asset}USDC`]?.last ||
      this.store.tickersBySymbol[`${asset}USD`]?.last ||
      0
    );
  };

  fetchOrders = async () => {
    const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.OPEN_ORDERS);

    const orders: Order[] = data.map((o) => {
      return {
        id: v(o, 'clientOrderId'),
        status: OrderStatus.Open,
        symbol: o.symbol,
        type: ORDER_TYPE_SPOT[o.type],
        side: ORDER_SIDE[o.side],
        price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')),
        amount: parseFloat(v(o, 'origQty')),
        reduceOnly: v(o, 'reduceOnly') || false,
        filled: parseFloat(v(o, 'executedQty')),
        lastUpdate: virtualClock.getCurrentTimeMs(),
        remaining: subtract(v(o, 'origQty'), v(o, 'executedQty')),
        info: o,
      };
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

    const { data } = await this.xhr.get<any[][]>(ENDPOINTS.KLINE, {
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

    const candles: Candle[] = data.map(([time, open, high, low, close, volume]) => {
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

  cancelOrders = async (orders: Order[]) => {
    try {
      const groupedBySymbol = groupBy(orders, 'symbol');
      const requests = Object.entries(groupedBySymbol).map(([symbol, symbolOrders]) => ({
        symbol,
        origClientOrderIdList: symbolOrders.map((o) => o.id),
      }));

      await forEachSeries(requests, async (request) => {
        for (const id of request.origClientOrderIdList) {
          await this.xhr.delete(ENDPOINTS.ORDER, {
            params: {
              symbol: request.symbol,
              origClientOrderId: id,
            },
          });
        }
        this.store.updateOrdersStatus(
          request.origClientOrderIdList.map((id) => ({ id })),
          OrderStatus.Canceled,
        );
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      await this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
        params: { symbol },
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
    const newOrder = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
    };

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
      throw new Error('Binance Spot trailing-stop placement requires trailingDelta and is not supported');
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);

    if (!Number.isFinite(amount) || amount <= 0) {
      const message = `Binance Spot orders require an explicit quantity greater than zero for ${opts.symbol}`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    // We use price only for limit orders
    // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    // Binance stopPrice only for SL or TP orders
    const priceField = isStopOrTP ? 'stopPrice' : 'price';

    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];
    const type =
      opts.type === OrderType.Limit
        ? opts.timeInForce === OrderTimeInForce.PostOnly
          ? 'LIMIT_MAKER'
          : 'LIMIT'
        : opts.type === OrderType.Market
          ? 'MARKET'
          : opts.type === OrderType.StopLoss
            ? 'STOP_LOSS'
            : opts.type === OrderType.TakeProfit
              ? 'TAKE_PROFIT'
              : undefined;

    if (!type) throw new Error(`Binance Spot does not support ${opts.type} orders`);

    const req = omitUndefined({
      symbol: opts.symbol,
      side: inverseObj(ORDER_SIDE)[opts.side],
      type,
      quantity: amount ? `${amount}` : undefined,
      [priceField]: price ? `${price}` : undefined,
      timeInForce:
        opts.type === OrderType.Limit && opts.timeInForce !== OrderTimeInForce.PostOnly ? timeInForce : undefined,
    });

    const shouldSplitLots = maxSize > 0 && amount > maxSize;
    const lots = shouldSplitLots ? Math.ceil(amount / maxSize) : 1;
    const rest = shouldSplitLots ? adjust(amount % maxSize, pAmount) : 0;

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
        type: 'STOP_LOSS',
        stopPrice: `${adjust(opts.stopLoss, pPrice)}`,
      });
    }

    if (opts.takeProfit) {
      payloads.push({
        ...omit(req, 'price'),
        side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
        type: 'TAKE_PROFIT',
        stopPrice: `${adjust(opts.takeProfit, pPrice)}`,
      });
    }

    // A unique ID per payload, otherwise Binance duplicates the IDs in batches.
    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
    for (const payload of payloads) {
      payload.newClientOrderId = clientOrderId ?? 'x-s3vFFRQp' + uuid();
    }

    return payloads;
  };

  private placeOrderBatch = async (payloads: any[]) => {
    const lots = chunk(payloads, 5);
    const orderIds = [] as string[];

    for (const lot of lots) {
      for (let i = 0; i < lot.length; i++) {
        try {
          await this.unlimitedXHR.post(ENDPOINTS.ORDER, lot[i]);
          orderIds.push(lot[i].newClientOrderId);
        } catch (err: any) {
          this.emitAndLogError(err?.response?.data?.msg || err?.message);
          throw err;
        }
      }
    }

    return orderIds;
  };
}
