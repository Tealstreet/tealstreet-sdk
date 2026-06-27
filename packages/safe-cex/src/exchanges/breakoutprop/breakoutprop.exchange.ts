import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';
import type { ExchangeName } from '../../types';

import rateLimit from 'axios-rate-limit';
import chunk from 'lodash/chunk';

import {
  Balance,
  Candle,
  ExchangeOptions,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderTimeInForce,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust } from '../../utils/safe-math';
import { uuid } from '../../utils/uuid';
import { BaseExchange } from '../base';
import { createAPI } from './breakoutprop.api';
import {
  CANDLE_RESPONSE,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INSTRUMENT_DETAILS_RESPONSE,
  INSTRUMENTS_RESPONSE,
  INTERVAL,
  LOGIN_RESPONSE,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
  PORTFOLIO_RESPONSE,
  POSITION_SIDE,
  TICKER_RESPONSE,
  USER_RESPONSE,
} from './breakoutprop.types';
import { BitruePrivateWebsocket } from './breakoutprop.ws-private';
import { BreakoutpropPublicWebsocket } from './breakoutprop.ws-public';

export class BreakoutpropExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BreakoutpropPublicWebsocket;
  privateWebsocket: BitruePrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  sessionToken: string | null = null;
  sessionTokenExpiresAt: number = 0;
  accountCode: string | null = null;

  recentWsRequests: Record<
    string,
    {
      time: number;
      data: Candle[];
    }
  > = {};

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 5 });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BreakoutpropPublicWebsocket(this);
    this.privateWebsocket = new BitruePrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  validateAccount = async () => {
    try {
      await this.getSessionToken();
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
    return { error: '' };
  };

  getSessionToken = async () => {
    if (this.sessionToken && Date.now() < this.sessionTokenExpiresAt) {
      return this.sessionToken;
    }
    const { data } = await this.xhr.post<LOGIN_RESPONSE>(ENDPOINTS.LOGIN, {
      domain: 'default',
      username: this.options.key,
      password: this.options.secret,
    });
    if (!data.sessionToken) {
      throw new Error('No session token received');
    }
    this.sessionToken = data.sessionToken;
    this.sessionTokenExpiresAt = Date.now() + 30 * 60 * 1000;
    return this.sessionToken;
  };

  getAccountCode = async (): Promise<string> => {
    try {
      if (this.accountCode) {
        return this.accountCode;
      }
      await this.getSessionToken();
      const { data } = await this.xhr.get<USER_RESPONSE>(ENDPOINTS.USER, {
        params: {
          username: this.options.key,
        },
      });
      const userDetail = data.userDetails[0];
      const validAccounts = userDetail.accounts.filter((account) => account.accountStatus === 'FULL_TRADING');
      if (!validAccounts.length) {
        throw new Error('No valid accounts found');
      }
      if (validAccounts.length > 1) {
        if (!this.options.subaccount) {
          throw new Error('Multiple accounts found. Please specify subaccount number ');
        }
        // account ==== default:{subaccount}
        const subaccount = validAccounts.find((account) => account.account.includes(this.options.subaccount!));
        if (!subaccount) {
          throw new Error('Subaccount for number not found');
        }
        this.accountCode = subaccount.account;
        return this.accountCode;
      } else {
        this.accountCode = validAccounts[0].account;
        return this.accountCode;
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  fetchInstrumentsRaw = async () => {
    try {
      await this.getSessionToken();
      const accountCode = await this.getAccountCode();
      const { data } = await this.xhr.get<INSTRUMENTS_RESPONSE>(ENDPOINTS.INSTRUMENTS, {
        params: {
          accountCode,
        },
      });
      return {
        // 'XBTUSD' is in their system but it causes 500 in ticker queries etc
        instruments: data.instruments.filter((instrument) => instrument.symbol !== 'XBTUSD'),
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return null;
    }
  };

  fetchInstrumentDetailsRaw = async () => {
    try {
      await this.getSessionToken();
      const accountCode = await this.getAccountCode();
      const { data } = await this.xhr.get<INSTRUMENT_DETAILS_RESPONSE>(ENDPOINTS.INSTRUMENT_DETAILS, {
        params: {
          accountCode,
        },
      });
      return {
        // 'XBTUSD' is in their system but it causes 500 in ticker queries etc
        instrumentDetails: data.instrumentDetails.filter((instrument) => instrument.symbol !== 'XBTUSD'),
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return null;
    }
  };

  fetchCombinedInstruments = async (): Promise<
    {
      instrument: INSTRUMENTS_RESPONSE['instruments'][0];
      instrumentDetails: INSTRUMENT_DETAILS_RESPONSE['instrumentDetails'][0];
    }[]
  > => {
    const [instruments, instrumentDetails] = await Promise.all([
      this.fetchInstrumentsRaw(),
      this.fetchInstrumentDetailsRaw(),
    ]);
    if (!instruments || !instrumentDetails) {
      return [];
    }
    const combinedInstruments = instrumentDetails.instrumentDetails.map((details) => {
      const instrument = instruments.instruments.find((i) => i.symbol === details.symbol);
      if (!instrument) {
        return null;
      }
      return {
        instrument,
        instrumentDetails: details,
      };
    });
    return combinedInstruments.filter(
      (
        i,
      ): i is {
        instrument: INSTRUMENTS_RESPONSE['instruments'][0];
        instrumentDetails: INSTRUMENT_DETAILS_RESPONSE['instrumentDetails'][0];
      } => i !== null,
    );
  };

  fetchMarkets = async () => {
    const data = await this.fetchCombinedInstruments();
    if (!data || data.length === 0) {
      return this.store.markets;
    }
    try {
      const markets: Market[] = data.map((instrument) => {
        const quote = 'USD';
        const base = instrument.instrument.symbol.replace(quote, '');
        return {
          id: instrument.instrument.symbol,
          symbol: instrument.instrument.symbol,
          base,
          quote,
          active: instrument.instrumentDetails.tradingStatus === 'FULL',
          precision: {
            amount: instrument.instrumentDetails.minOrderSize,
            price: -Math.log10(instrument.instrument.priceIncrement),
          },
          limits: {
            amount: {
              min: instrument.instrumentDetails.minOrderSize,
              max: instrument.instrumentDetails.maxOrderSize.value,
            },
            leverage: {
              min: 1,
              max: 1 / instrument.instrumentDetails.marginRate.tiers[0].value,
            },
          },
          contractSize: instrument.instrument.lotSize,
          isExpiring: false,
          isInverse: false,
          isSpot: false,
          isReadOnly: false,
          info: instrument,
          takerFee: 0,
          makerFee: 0,
        };
      });
      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    return await this.fetchTickersForSymbols(this.store.markets.map((m) => m.symbol));
  };

  fetchTickersForSymbols = async (symbols: string[]): Promise<Ticker[]> => {
    try {
      await this.getSessionToken();
      const accountCode = await this.getAccountCode();
      // export type TICKER_RESPONSE = {
      //   symbol: string;
      //   type: string;
      //   ask: number;
      //   bid: number;
      //   time: string;
      // }[];
      const { data } = await this.xhr.post<TICKER_RESPONSE>(ENDPOINTS.MARKET_DATA, {
        eventTypes: [
          {
            type: 'Quote',
          },
        ],
        symbols,
        account: accountCode,
      });
      const tickers: Ticker[] = data.events?.map((rawTicker) => ({
        id: rawTicker.symbol,
        symbol: rawTicker.symbol,
        bid: rawTicker.bid,
        ask: rawTicker.ask,
        last: rawTicker.bid,
        mark: rawTicker.bid,
        index: rawTicker.bid,
        percentage: 0,
        openInterest: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
        info: rawTicker,
      }));

      return tickers;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  fetchPortfolioRaw = async () => {
    try {
      await this.getSessionToken();
      const accountCode = await this.getAccountCode();
      const { data } = await this.xhr.get<PORTFOLIO_RESPONSE>(ENDPOINTS.PORTFOLIO, {
        params: {
          accountCode,
        },
      });
      return data;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return null;
    }
  };

  formatBalance = (data: PORTFOLIO_RESPONSE) => {
    try {
      const usdBalance = data.portfolios[0]?.balances?.[0]?.value || 0;
      const balance: Balance = {
        total: usdBalance,
        free: usdBalance,
        upnl: 0,
        used: 0,
        info: data,
      };
      return balance;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.balance;
    }
  };

  fetchBalance = async () => {
    const data = await this.fetchPortfolioRaw();
    if (!data) {
      return this.store.balance;
    }
    const balance = this.formatBalance(data);
    return balance;
  };

  formatOpenPositions = (data: PORTFOLIO_RESPONSE) => {
    try {
      const positions: Position[] = data.portfolios[0]?.positions.map((position) => ({
        symbol: position.symbol,
        side: POSITION_SIDE[position.side],
        entryPrice: position.openPrice,
        notional: position.quantityNotional,
        leverage: 1 / position.marginRate,
        unrealizedPnl: 0,
        contracts: position.quantity,
        liquidationPrice: 0,
        id: createPositionId(position.symbol),
        positionMode: PositionMode.OneWay,
        marginType: MarginType.Cross,
        info: position,
      }));
      return positions;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.positions;
    }
  };

  formatPositions = (data: PORTFOLIO_RESPONSE, symbols?: string[]) => {
    const openPositions = this.formatOpenPositions(data);
    if (!symbols) {
      symbols = this.store.markets.map((m) => m.symbol);
    }
    const openPositionsForSymbols = openPositions.filter((p) => symbols.includes(p.symbol));
    const missingSymbols = symbols.filter((s) => !openPositionsForSymbols.some((p) => p.symbol === s));
    const fakePositions: Position[] = missingSymbols.map((symbol) => {
      return {
        id: createPositionId(symbol),
        symbol,
        side: PositionSide.Long,
        leverage: 0,
        marginType: MarginType.Cross,
        positionMode: PositionMode.OneWay,
        entryPrice: 0,
        notional: 0,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
      };
    });
    return [...openPositionsForSymbols, ...fakePositions];
  };

  fetchPositions = async (symbols?: string[]) => {
    const data = await this.fetchPortfolioRaw();
    if (!data) {
      return this.store.positions;
    }
    return this.formatPositions(data, symbols);
  };

  fetchOpenPositions = async () => {
    const data = await this.fetchPortfolioRaw();
    if (!data) {
      return this.store.positions;
    }
    return this.formatOpenPositions(data);
  };

  fetchOrders = async () => {
    const { data } = await this.xhr.get<Record<string, Record<string, Array<Record<string, any>>>>>(ENDPOINTS.ORDERS);

    console.log(data);

    return [];

    // if (data.code) {
    //   this.emitAndLogError(`${data.msg}`);
    //   return this.store.orders;
    // }
    //
    // const orders: Order[] = data.data.orders.map((o) => {
    //   const order = {
    //     id: v(o, 'orderId'),
    //     status: OrderStatus.Open,
    //     symbol: o.symbol.replace(/-/, ''),
    //     type: ORDER_TYPE[o.type],
    //     side: ORDER_SIDE[o.side],
    //     price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')),
    //     amount: parseFloat(v(o, 'origQty')),
    //     reduceOnly: v(o, 'reduceOnly') || false,
    //     timeInForce: v(o, 'postOnly') ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
    //     filled: parseFloat(v(o, 'executedQty')),
    //     lastUpdate: virtualClock.getCurrentTimeMs(),
    //     remaining: subtract(v(o, 'origQty'), v(o, 'executedQty')),
    //     info: o,
    //     ...(ORDER_TYPE[o.type] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
    //     ...(ORDER_TYPE[o.type] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
    //   };
    //   if (order.type === OrderType.StopLoss) {
    //     const ticker = this.store.tickers.find((t) => t.symbol === order.symbol);
    //     if (ticker) {
    //       if (
    //         (order.side === OrderSide.Buy && order.price < ticker.mark) ||
    //         (order.side === OrderSide.Sell && order.price > ticker.mark)
    //       ) {
    //         order.type = OrderType.TakeProfit;
    //         order.tpTriggerType = order.slTriggerType;
    //       }
    //     }
    //   }
    //   return order;
    // });

    // return orders;
  };

  formatOrders = (data: PORTFOLIO_RESPONSE) => {
    // orders: {
    //   account: string;
    //   version: number;
    //   orderId: number;
    //   orderCode: string;
    //   clientOrderId: string;
    //   actionCode: string;
    //   legCount: number;
    //   type: string;
    //   instrument: string;
    //   status: string;
    //   finalStatus: boolean;
    //   legs: {
    //     instrument: string;
    //     positionEffect: string;
    //     positionCode: string;
    //     side: string;
    //     price: number;
    //     legRatio: number;
    //     quantity: number;
    //     filledQuantity: number;
    //     remainingQuantity: number;
    //     averagePrice: number;
    //   }[];
    try {
      const orders: Order[] = data.portfolios[0]?.orders.map((order) => ({
        id: order.orderCode,
        status: ORDER_STATUS[order.status],
        symbol: order.instrument,
        type: ORDER_TYPE[order.type],
        side: ORDER_SIDE[order.legs[0].side],
        price: order.legs[0].price,
        amount: order.legs[0].quantity,
        filled: order.legs[0].filledQuantity,
        remaining: order.legs[0].remainingQuantity,
        reduceOnly: false,
        info: order,
        timeInForce: OrderTimeInForce.GoodTillCancel,
        lastUpdate: order.legs[0].price,
      }));
      return orders;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.orders;
    }
  };

  cancelOrder = async (orderCode: string) => {
    try {
      await this.getSessionToken();
      const accountCode = await this.getAccountCode();
      await this.xhr.delete(ENDPOINTS.CANCEL_ORDER, {
        params: {
          accountCode,
          orderCode,
        },
      });
      return orderCode;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelOrders = async (orders: Order[]) => {
    const lots = chunk(orders, 10);
    const results = [];
    let error = null;
    for (const lot of lots) {
      const innerResults = await Promise.all(
        lot.map((order) => {
          try {
            this.cancelOrder(order.id);
          } catch (err: any) {
            error = err;
            return null;
          }
        }),
      );
      results.push(...innerResults);
    }
    if (error) {
      throw error;
    }
    return;
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const newOrder = {
      ...order,
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      ...(order.reduceOnly ? { reduceOnly: true } : {}),
    };

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    await this.cancelOrders([order]);
    return await this.placeOrder(newOrder);
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    try {
      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error('Market not found');
      }

      await this.getSessionToken();
      const accountCode = await this.getAccountCode();

      // export type PlaceOrderOpts = {
      //   readonly symbol: string;
      //   readonly type: OrderType;
      //   readonly side: OrderSide;
      //   readonly amount: number;
      //   readonly price?: number;
      //   readonly triggerPrice?: number;
      //   readonly stopLoss?: number;
      //   readonly takeProfit?: number;
      //   readonly reduceOnly?: boolean;
      //   readonly timeInForce?: OrderTimeInForce;
      //   readonly tpTriggerType?: OrderStopTriggerType;
      //   readonly slTriggerType?: OrderStopTriggerType;
      // };

      const pPrice = market.precision.price;
      const pAmount = market.precision.amount;

      const amount = adjust(opts.amount, pAmount);
      const price = opts.price ? adjust(opts.price, pPrice) : null;

      // orderCode	Unique order code that identifies the order on an account	String, required
      // type	Order type	String, required
      // instrument	Symbol of the instrument	String, required
      // quantity	Initial quantity of the order in units (not in lots)	Number, optional
      // positionEffect	The effect of this order on a position	String, optional
      // positionCode	ID of a position that should be modified by this order	String, optional
      // side	Order operation	String, required
      // limitPrice	Limit order price per unit of quantity. (use stopPrice for stop orders). Should be absent for Market and Stop orders	Number, optional
      // stopPrice	Stop order price per unit of quantity. (use limitPrice for limit orders). Should be absent for Market and Limit orders	Number, optional
      // priceOffset	Price offset for protection orders	Number, optional
      // priceLink	Price link for protection orders	String, optional
      // tif	Time in force (expiration time for the order)	String, optional
      // expireDate	Order expiration date in UTC	Date, optional
      const req = omitUndefined({
        orderCode: `tealstreet-${uuid()}`,
        type: inverseObj(ORDER_TYPE)[opts.type],
        instrument: opts.symbol,
        quantity: amount,
        // positionEffect	The effect of this order on a position	String, optional
        // positionCode	ID of a position that should be modified by this order	String, optional
        side: inverseObj(ORDER_SIDE)[opts.side],
        limitPrice: price,
        // stopPrice	Stop order price per unit of quantity. (use limitPrice for limit orders). Should be absent for Market and Limit orders	Number, optional
        positionEffect: 'OPEN',
        // priceLink	Price link for protection orders	String, optional
        tif: 'GTC',
        // expireDate	Order expiration date in UTC	Date, optional
      });

      const { data } = await this.xhr.post(ENDPOINTS.PLACE_ORDER, req, {
        params: {
          accountCode,
        },
      });
      if (!data.orderId) {
        throw new Error('No order ID received');
      }
      return data.orderId;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!this.isPublicDataOnly) {
      await this.getAccountCode();
    }

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;
      this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;
      this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    const portfolioRaw = await this.fetchPortfolioRaw();
    if (this.isDisposed) return;
    if (portfolioRaw) {
      const balance = this.formatBalance(portfolioRaw);
      const positions = this.formatPositions(portfolioRaw);
      const orders = this.formatOrders(portfolioRaw);
      this.store.update({
        balance,
        positions,
        orders,
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
    }

    await this.tick();

    // const publicDataOnly = this.options.publicDataOnly || !this.options.key || !this.options.secret;
    // const markets = await this.fetchMarkets();
    // if (this.isDisposed) return;
    // // create empty ticker stubs so we can use deltas right away in ws
    // const tickers: Ticker[] = markets.map((m) => ({
    //   id: m.symbol,
    //   symbol: m.symbol,
    //   bid: 0,
    //   ask: 0,
    //   last: 0,
    //   mark: 0,
    //   index: 0,
    //   percentage: 0,
    //   openInterest: 0,
    //   fundingRate: 0,
    //   fundingTime: 0,
    //   volume: 0,
    //   quoteVolume: 0,
    // }));
    // this.store.update({
    //   markets,
    //   tickers,
    //   loaded: { ...this.store.loaded, markets: true, tickers: true },
    // });
    //
    // this.publicWebsocket.connectAndSubscribe();
    //
    // if (!publicDataOnly) {
    //   // this.privateWebsocket.connectAndSubscribe();
    //   const balance = await this.fetchBalance();
    //   if (this.isDisposed) return;
    //   this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
    //   const orders = await this.fetchOrders();
    //   if (this.isDisposed) return;
    //   this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
    // }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    try {
      await this.getSessionToken();
      const accountCode = await this.getAccountCode();

      // https://demo.dx.trade/developers/#/DXtrade-REST-API?id=market-data-request

      // type	Either Quote or Candle	Account code, required
      // format	Always COMPACT	String, optional
      // candleType	Type of the candle, only for candle queries	String, optional
      // fromTime	Required for Candle type. UTC time to get history from.	Timestamp, optional
      // toTime	Required for Candle type. UTC time to get history till.	Timestamp, optional
      // count	Maximum count of candles in response. If not defined, used value predefined on server side.	Number, optional

      // {
      //   "events": [
      //   {
      //     "symbol": "BTCUSD",
      //     "type": "Candle",
      //     "candleType": "h",
      //     "open": 98117.9,
      //     "close": 98035.9,
      //     "high": 98173.6,
      //     "low": 97952,
      //     "volume": 922.778,
      //     "time": "2024-12-11T09:00:00Z"
      //   }
      // ]
      // }
      const { data } = await this.xhr.post<CANDLE_RESPONSE>(ENDPOINTS.MARKET_DATA, {
        eventTypes: [
          {
            type: 'Candle',
            candleType: INTERVAL[opts.interval],
            // 2013-01-24T23:05:08.52Z
            fromTime: opts.from && new Date(opts.from).toISOString(),
            toTime: opts.to && new Date(opts.to).toISOString(),
          },
        ],
        symbols: [opts.symbol],
        account: accountCode,
      });
      const candles: Candle[] = data.events.map((rawCandle) => ({
        timestamp: new Date(rawCandle.time).getTime(),
        open: rawCandle.open,
        high: rawCandle.high,
        low: rawCandle.low,
        close: rawCandle.close,
        volume: rawCandle.volume,
      }));
      return candles;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  tick = async () => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        const portfolioRaw = await this.fetchPortfolioRaw();
        if (this.isDisposed) return;
        if (portfolioRaw) {
          const balance = this.formatBalance(portfolioRaw);
          const positions = this.formatPositions(portfolioRaw);
          const orders = this.formatOrders(portfolioRaw);
          this.store.update({
            balance,
            positions,
            orders,
            loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
          });
        }

        // await this.fetchAndSetPrivateSnapshotHelper(false);
        // iterate over tickers and update OB and kline callbacks
        const tickers = await this.fetchTickers();
        if (this.isDisposed) return;
        this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
        const messageHandlers = this.publicWebsocket.messageHandlers;
        tickers.forEach((ticker) => {
          if (messageHandlers[`poll_book.${ticker.symbol}`]) {
            messageHandlers[`poll_book.${ticker.symbol}`].forEach((cb) => cb(ticker));
          }
          if (messageHandlers[`poll_kline.${ticker.symbol}`]) {
            messageHandlers[`poll_kline.${ticker.symbol}`].forEach((cb) => cb(ticker));
          }
        });
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };
}
