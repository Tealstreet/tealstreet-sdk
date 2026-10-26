import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';
import type { Balance, ExchangeName } from '../../../types';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import capitalize from 'lodash/capitalize';
import chunk from 'lodash/chunk';
import groupBy from 'lodash/groupBy';
import sumBy from 'lodash/sumBy';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';
import { mapSeries } from 'p-iteration';

import {
  Candle,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
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
import { roundUSD } from '../../../utils/roundUSD';
import { adjust, multiply1, subtract } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { BROKER_ID, createAPI } from './phemex.api';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
  POSITIONS_RESPONSE,
  TIME_IN_FORCE,
} from '../../../exchanges/phemex/phemex.types';
import { BinancePrivateWebsocket } from './phemex.ws-private';
import { BinancePublicWebsocket } from './phemex.ws-public';

export class PhemexExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Phemex', /^.{1,40}$/, {
    mint: () => `${BROKER_ID}_${uuid()}`,
  });

  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BinancePublicWebsocket;
  privateWebsocket: BinancePrivateWebsocket;

  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;
  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;

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

  getAccount = async () => {
    const { data } = await this.xhr.get<{ data: POSITIONS_RESPONSE }>(ENDPOINTS.ACCOUNT, {
      params: { currency: 'USDT' },
    });
    const userID = data?.data?.account?.userID;

    if (userID === undefined || userID === null) {
      const msg = 'Phemex account-position response did not include a user ID';
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    return { userId: String(userID) };
  };

  validateAccount = async () => {
    let balance: Balance | undefined;
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT, {
        params: {
          currency: 'USDT',
        },
      });
      // Same ACCOUNT response fetchPositionsAndBalance parses (account.accountBalanceRv).
      const account = data?.data?.account;
      const total = parseFloat(account?.accountBalanceRv);
      if (Number.isFinite(total)) {
        const used = parseFloat(account?.totalUsedBalanceRv) || 0;
        balance = { total, used, free: total - used, upnl: 0 };
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
    return { error: '', balance };
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    if (!useExternalPublicData) {
      const [, rawTickers] = await Promise.all([
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
      const tickers = this.parseTickers(rawTickers);
      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });
    }

    if (!publicDataOnly) {
      await this._startPrivate();
    } else {
      await this._startPublicDataOnly();
    }
  };

  _startPrivate = async () => {
    this.privateWebsocket.connectAndSubscribe();

    // fetch current position mode (Hedge/One-way)
    this.store.setSetting('isHedged', true);

    // start ticking live data
    // balance, tickers, positions
    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Phemex`);

    // not needed as we really are getting snapshot from ws...

    // // fetch unfilled orders
    // const orders = await this.fetchOrders();
    // if (this.isDisposed) return;
    //
    // this.log(`Loaded Phemex orders`);
    //
    // this.store.update({
    //   orders,
    //   loaded: { ...this.store.loaded, orders: true },
    // });
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
      const revision = this.privateWebsocket.accountStateRevision;
      try {
        const { balance, positions } = await this.fetchPositionsAndBalance();
        if (this.isDisposed) return;
        // A private push received while polling owns the newer account state.
        this.privateWebsocket.commitAccountSnapshot(revision, balance, positions);
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
        data: {
          data: { perpProductsV2 },
        },
      } = await this.xhr.get<{ data: { perpProductsV2: Array<Record<string, any>> } }>(ENDPOINTS.MARKETS);

      const markets: Market[] = perpProductsV2
        .filter((m: any) => v(m, 'type') === 'PerpetualV2' && ['USDT', 'USDC'].includes(v(m, 'settleCurrency')))
        .map((m: any) => {
          const baseAsset = v(m, 'baseCurrency');
          const quoteAsset = v(m, 'quoteCurrency');
          const marginAsset = v(m, 'settleCurrency');

          return {
            id: `${baseAsset}/${quoteAsset}:${marginAsset}`,
            symbol: this.formatSymbolIn(m.symbol),
            base: baseAsset,
            quote: quoteAsset,
            active: true,
            precision: {
              amount: parseFloat(v(m, 'qtyStepSize')),
              price: parseFloat(v(m, 'tickSize')),
            },
            limits: {
              amount: {
                min: 0,
                max: Math.min(parseFloat(v(m, 'maxOrderQtyRq')), parseFloat(v(m, 'maxOrderQtyRq'))),
              },
              leverage: {
                min: 1,
                max: parseFloat(v(m, 'maxLeverage')),
              },
            },
            info: m,
          };
        });

      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.msg || err?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    const data = await this.fetchTickersRaw();
    return this.parseTickers(data);
  };

  fetchTickersRaw = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.TICKERS);
    if (v(data, 'error') !== null) {
      this.emitAndLogError(v(data, 'error'));
      return null;
    }
    return data;
  };

  parseTickers = (data: any) => {
    if (!data || !data.result) {
      return this.store.tickers;
    }

    const tickers: Ticker[] = data.result.reduce((acc: Ticker[], t: Record<string, any>) => {
      const symbol = this.formatSymbolIn(t.symbol);
      const market = this.store.marketsBySymbol[symbol];

      if (!market) return acc;
      if (!t.symbol.endsWith('USDT') && !t.symbol.endsWith('USDC')) return acc;

      const ticker = {
        id: market.id,
        symbol,
        bid: parseFloat(t.bidRp),
        ask: parseFloat(t.askRp),
        last: parseFloat(t.lastRp),
        mark: parseFloat(t.markRp),
        index: parseFloat(t.indexRp),
        percentage: ((parseFloat(t.lastRp) - parseFloat(t.openRp)) / parseFloat(t.openRp)) * 100,
        openInterest: parseFloat(t.openInterestRv),
        fundingRate: parseFloat(t.fundingRateRr),
        fundingTime: 0,
        volume: parseFloat(t.volumeRq),
        quoteVolume: parseFloat(t.volumeRq) * parseFloat(t.lastRp),
      };

      return [...acc, ticker];
    }, []);

    return tickers;
  };

  calculatePriceDiff = (price: number, markPrice: number, side: PositionSide) => {
    if (side === PositionSide.Long) {
      return subtract(markPrice, price);
    } else {
      return subtract(price, markPrice);
    }
  };

  mapPosition = (p: POSITIONS_RESPONSE['positions'][0]): Position | undefined => {
    const entryPrice = parseFloat(v(p, 'avgEntryPrice') || v(p, 'avgEntryPriceRp'));
    const size = parseFloat(v(p, 'size'));
    if (!Number.isFinite(size) || size === 0) return undefined;
    if (!Number.isFinite(entryPrice)) return undefined;

    const symbol = this.formatSymbolIn(p.symbol);
    const ticker = this.store.tickersBySymbol[symbol];
    const markPrice = parseFloat(v(p, 'markPriceRp'));
    const lastPrice = Number.isFinite(ticker?.last)
      ? ticker!.last
      : Number.isFinite(markPrice)
        ? markPrice
        : entryPrice;

    // we probably need this for COIN-M, but USD-M is not contracts based
    //const contracts = size ? multiply1(size, market.precision.amount) : 0;
    const contracts = size;
    const rawPosSide = v(p, 'posSide');
    const rawSide = v(p, 'side') || v(p, 'Side');
    const pSide = rawPosSide === 'Long' || rawSide === 'Buy' ? PositionSide.Long : PositionSide.Short;
    //const upnl = multiply1(contracts, this.calculatePriceDiff(entryPrice, parseFloat(v(p, 'markPriceRp')), pSide));
    const upnl = multiply1(contracts, this.calculatePriceDiff(entryPrice, lastPrice, pSide));

    // If account is not on hedge mode,
    // we need to define the side of the position with the contracts amount

    const rawLeverage = parseFloat(p.leverageRr);
    const positionMode = p.posMode === 'Hedged' ? PositionMode.Hedged : PositionMode.OneWay;
    const marginType = rawLeverage < 0 || p.crossMargin ? MarginType.Cross : MarginType.Isolated;

    return {
      id: createPositionId(symbol, pSide, positionMode),
      symbol,
      side: pSide,
      entryPrice,
      notional: Math.abs(contracts) * entryPrice,
      leverage: Math.abs(rawLeverage),
      unrealizedPnl: upnl,
      contracts: Math.abs(contracts),
      liquidationPrice: parseFloat(v(p, 'liquidationPriceRp')),
      // marginType: rawLeverage < 0 ? MarginType.Isolated : MarginType.Cross,
      marginType,
      positionMode,

      info: p,
    };
  };

  fetchPositions = async (symbols?: string[]) => {
    if (!symbols) {
      const { positions } = await this.fetchPositionsAndBalance();
      return positions;
    }
    return mapSeries(symbols, this.fetchPositionForSymbol).then((positions) => positions.flat());
  };

  fetchPositionForSymbol = async (symbol: string) => {
    try {
      const {
        data: { data },
      } = await this.xhr.get<{ data: POSITIONS_RESPONSE }>(ENDPOINTS.POSITIONS, {
        params: {
          currency: symbol?.endsWith('USDC') ? 'USDC' : 'USDT',
          symbol: this.formatSymbolOut(symbol),
        },
      });
      if (!data) return [];
      const parsedPositions: Position[] = (data.positions || [])
        .map(this.mapPosition.bind(this))
        .filter((position: Position | undefined): position is Position => Boolean(position));
      return parsedPositions;
    } catch (e: any) {
      this.emitAndLogError(e, e?.response?.data?.msg || e?.message);
      return [];
    }
  };

  fetchPositionsAndBalance = async () => {
    try {
      const currencies = ['USDT', 'USDC'] as const;
      const responses = await Promise.allSettled(
        currencies.map(async (currency) => {
          const response = await this.xhr.get<Record<string, any>>(ENDPOINTS.ACCOUNT, {
            params: { currency },
          });
          const data = response.data?.data;
          if (response.data?.code !== 0 || !data?.account || !Array.isArray(data.positions)) {
            throw new Error(response.data?.msg || `Invalid ${currency} account-position response`);
          }
          return { currency, ...data };
        }),
      );

      const accounts: Record<string, any> = {};
      const currencyBalances: NonNullable<Balance['currencies']> = {};
      const parsedPositions: Position[] = [];
      const failedCurrencies = new Set<string>();

      for (let i = 0; i < currencies.length; i++) {
        const currency = currencies[i];
        const response = responses[i];
        if (response.status === 'rejected') {
          failedCurrencies.add(currency);
          this.emitAndLogError(
            response.reason?.response?.data?.msg || response.reason?.message || String(response.reason),
          );
          continue;
        }

        const { account, positions } = response.value;
        accounts[currency] = account;

        const currencyPositions: Position[] = [];
        for (let j = 0; j < positions.length; j++) {
          const position = positions[j];
          if (!this.store.marketsBySymbol[this.formatSymbolIn(position.symbol)]) continue;

          const parsedPosition = this.mapPosition(position);
          if (!parsedPosition) continue;

          currencyPositions.push(parsedPosition);
          parsedPositions.push(parsedPosition);
        }

        const total = parseFloat(account.accountBalanceRv);
        const used = parseFloat(account.totalUsedBalanceRv);
        const upnl = roundUSD(sumBy(currencyPositions, (p) => parseFloat(String(p.unrealizedPnl))));
        currencyBalances[currency] = {
          // Keep REST and account pushes on the same normalized balance convention.
          total: subtract(total, upnl),
          used,
          free: subtract(total, used),
          upnl,
          info: account,
        };
      }

      if (Object.keys(currencyBalances).length === 0) {
        return { balance: this.store.balance, positions: this.store.positions };
      }

      for (const currency of failedCurrencies) {
        const previousBalance = this.store.balance.currencies?.[currency];
        if (previousBalance) {
          currencyBalances[currency] = previousBalance;
        } else {
          // getBalanceSlice falls back to the aggregate for a missing currency.
          // An unavailable settlement account must therefore fail closed instead.
          currencyBalances[currency] = { total: 0, used: 0, free: 0, upnl: 0 };
        }
        parsedPositions.push(
          ...this.store.positions.filter((position) => {
            const market = this.store.marketsBySymbol[position.symbol];
            return (
              market?.quote === currency ||
              market?.info?.settleCurrency === currency ||
              position.info?.currency === currency
            );
          }),
        );
      }

      // Both supported settlement assets are USD stablecoins. Per-market consumers
      // read balance.currencies; this aggregate is the nominal USD-stable total.
      const totalCollateral = Object.values(currencyBalances).reduce((total, balance) => total + balance.total, 0);
      const used = Object.values(currencyBalances).reduce((total, balance) => total + balance.used, 0);
      const upnl = roundUSD(sumBy(parsedPositions, (p) => parseFloat(String(p.unrealizedPnl))));

      const balance: Balance = {
        used,
        free: sumBy(Object.values(currencyBalances), 'free'),
        total: totalCollateral,
        upnl,
        info: accounts,
        currencies: currencyBalances,
      };

      return { balance, positions: parsedPositions };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { balance: this.store.balance, positions: this.store.positions };
    }
  };

  // need to either implement or return orders as some err handling does refetch / overwrite of orders...
  // TODO: fetchAndSetPrivateSnapshot
  fetchOrders = async () => {
    return this.store.orders;
  };

  mapOrderUpdate = (order: Record<string, any>): Order[] => {
    const rawOrderStatus = `${order.ordStatus || order.orderStatus || ''}`.toLowerCase();
    let status = ORDER_STATUS[rawOrderStatus] || OrderStatus.Canceled;

    // Market, Limit, Stop, StopLimit, MarketIfTouched, LimitIfTouched.
    const rawOrderType = order.ordType || order.orderType;
    let type = OrderType.Market;
    if (rawOrderType === 'Market') {
      type = OrderType.Market;
    } else if (rawOrderType === 'Limit') {
      type = OrderType.Limit;
    } else if (
      rawOrderType === 'Stop' ||
      rawOrderType === 'StopLimit' ||
      rawOrderType === 'MarketIfTouched' ||
      rawOrderType === 'LimitIfTouched'
    ) {
      if (order.stopDirection === 'Rising') {
        type = order.side === 'Sell' ? OrderType.TakeProfit : OrderType.StopLoss;
      } else if (order.stopDirection === 'Falling') {
        type = order.side === 'Sell' ? OrderType.StopLoss : OrderType.TakeProfit;
      }
    }

    const timeInForce = TIME_IN_FORCE[order.timeInForce];
    const reduceOnly =
      order.reduceOnly === true ||
      order.reduceOnly === 'true' ||
      ['ReduceOnly', 'CloseOnTrigger'].some((i) => order.execInst?.includes(i));
    const amount = parseFloat(v(order, 'orderQty') || v(order, 'orderQtyRq') || '0');
    const filled = status === OrderStatus.Canceled ? 0 : parseFloat(v(order, 'execQty') || '0') || 0;
    const remaining =
      parseFloat(v(order, 'leavesQty')) || parseFloat(v(order, 'orderQty')) || parseFloat(v(order, 'orderQtyRq')) || 1;
    const trigger = v(order, 'trigger') || v(order, 'triggerType');

    const formattedOrders = [
      {
        id: order.orderID,
        status,
        symbol: this.formatSymbolIn(order.symbol),
        type,
        side: order.side === 'Buy' ? OrderSide.Buy : OrderSide.Sell,
        price: parseFloat(v(order, 'stopPxRp')) || parseFloat(v(order, 'priceRp')),
        amount,
        reduceOnly,
        filled,
        // Stops use orderQty/orderQtyRq because leavesQty is absent before trigger.
        remaining,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        info: order,
        timeInForce,
        ...(type === OrderType.TakeProfit && {
          tpTriggerType: trigger === 'ByLastPrice' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice,
        }),
        ...(type === OrderType.StopLoss && {
          slTriggerType: trigger === 'ByLastPrice' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice,
        }),
      },
    ];

    const sl = parseFloat(v(order, 'stopLossRp'));
    const tp = parseFloat(v(order, 'takeProfitRp'));
    const inverseSide = formattedOrders[0].side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;

    if (sl > 0) {
      formattedOrders.push({
        ...formattedOrders[0],
        id: `${order.orderID}__stop_loss`,
        type: OrderType.StopLoss,
        side: inverseSide,
        price: sl,
        filled: 0,
        amount: 0,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        slTriggerType:
          v(order, 'slTrigger') === 'ByLastPrice' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice,
      });
    }

    if (tp > 0) {
      formattedOrders.push({
        ...formattedOrders[0],
        id: `${order.orderID}__take_profit`,
        type: OrderType.TakeProfit,
        side: inverseSide,
        price: tp,
        filled: 0,
        amount: 0,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        tpTriggerType:
          v(order, 'tpTrigger') === 'ByLastPrice' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice,
      });
    }

    return formattedOrders;
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 100 } = opts;
    if (!symbol) {
      return [];
    }

    const market = this.store.marketsBySymbol[symbol];
    if (!market) {
      return [];
    }

    const maxLimit = Math.min(Math.max(limit, 1), 200);
    const apiSymbol = this.formatSymbolOut(symbol);

    const normalizeToMilliseconds = (value?: number) => {
      if (value === undefined || value === null || !Number.isFinite(value)) {
        return undefined;
      }
      return value > 1_000_000_000_000 ? Math.floor(value) : Math.floor(value * 1000);
    };

    const normalizeToSeconds = (value: unknown) => {
      if (value === undefined || value === null) {
        return undefined;
      }
      if (typeof value === 'string' && !/^\d+$/.test(value)) {
        const parsedDate = Date.parse(value);
        return Number.isFinite(parsedDate) ? Math.floor(parsedDate / 1000) : undefined;
      }
      const parsed = Number.parseInt(String(value), 10);
      if (!Number.isFinite(parsed) || parsed <= 0) {
        return undefined;
      }
      if (parsed > 1_000_000_000_000_000) {
        return Math.floor(parsed / 1_000_000_000);
      }
      if (parsed > 1_000_000_000_000) {
        return Math.floor(parsed / 1000);
      }
      return parsed;
    };

    const fromMilliseconds = normalizeToMilliseconds(from);
    const toMilliseconds = normalizeToMilliseconds(to);

    const parseRows = (rows: Array<Record<string, any>>) =>
      rows.reduce<Execution[]>((acc, row) => {
        // MEASURED 2026-08-14 on sheet row 24: this route returns NO `execType` field, and
        // `tradeType` is a NUMBER (`1`), not a word. The old test stringified it to `'1'`,
        // found it !== `'trade'`, and dropped EVERY row — so even a successful response
        // produced zero executions.
        //
        // Only a recognisable STRING may exclude a row. An undecoded numeric code must not
        // silently discard data: we have not established what phemex's `tradeType` enum
        // means, and dropping rows on a value we cannot read is how an axis reads as empty.
        const rawExecType = row.execType ?? row.tradeType;
        if (typeof rawExecType === 'string' && rawExecType && rawExecType.toLowerCase() !== 'trade') {
          return acc;
        }

        const rowSymbol = this.formatSymbolIn(row.symbol || row.contract || '');
        if (rowSymbol !== market.symbol) {
          return acc;
        }

        // **BLOCKER, deliberately not guessed.** This route returns `side` as a NUMBER (`1`),
        // not `Buy`/`Sell`. `String(1).toUpperCase()` is `'1'`, matches neither, and drops the
        // row — which is why a 200-row page produced zero executions.
        //
        // The numeric enum is NOT decoded here. Reading it off phemex's docs is exactly what
        // produced the `execType` defect above: that parameter was doc-derived, had never been
        // called, and was rejected outright by the venue. An inverted side is also the one
        // error every reconciliation still passes (see the side invariant), so a wrong guess
        // here would be invisible rather than loud.
        //
        // To settle it: correlate a known e2e order's side against its exec row, the same
        // ground-truth method used for the fold grader. Until then a numeric side is left
        // unmapped and the row is skipped, which is honest rather than silently inverted.
        const sideRaw = String(row.side ?? row.execSide ?? '').toUpperCase();
        const side = sideRaw === 'BUY' ? OrderSide.Buy : sideRaw === 'SELL' ? OrderSide.Sell : undefined;
        if (!side) {
          return acc;
        }

        const price = parseFloat(row.execPriceRp || row.execPrice || row.price || row.avgPrice || '0');
        // `execQtyRq` is what this route actually returns — measured, row 24. None of the four
        // names below it exist on the response, so every row parsed to qty 0 and was dropped
        // by the guard underneath. Phemex's V2 fields carry the Rq/Rp/Rv suffixes; the older
        // unsuffixed names are kept only as a fallback for the legacy route.
        const qty = parseFloat(row.execQtyRq || row.execQty || row.lastQty || row.qty || row.size || '0');
        if (!Number.isFinite(price) || !Number.isFinite(qty) || price <= 0 || qty <= 0) {
          return acc;
        }

        // `createdAt` (milliseconds) is this route's timestamp — measured. None of the other
        // names are present, so every row resolved to no timestamp and was dropped.
        const timestamp = normalizeToSeconds(
          row.createdAt || row.execTime || row.createTime || row.transactTimeNs || row.transactTime || row.timestamp,
        );
        if (!timestamp) {
          return acc;
        }

        const executionOrderId = String(row.orderID || row.orderId || row.execID || row.execId || '');
        if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
          return acc;
        }

        const fee = parseFloat(row.execFeeRv || row.execFee || row.fee || '0');
        acc.push(
          omitUndefined({
            // execID is read above only as an orderId fallback; on its own it is the
            // per-fill dedupe key, which orderId cannot be.
            id: row.execID || row.execId ? String(row.execID || row.execId) : undefined,
            timestamp,
            symbol: market.symbol,
            orderId: executionOrderId || undefined,
            price,
            qty,
            side,
            commission: Number.isFinite(fee) ? Math.abs(fee) : undefined,
            commissionAsset: row.feeCurrency || row.currency || market.quote,
          }),
        );
        return acc;
      }, []);

    const extractRows = (payload: any): Array<Record<string, any>> => {
      const body = payload?.data ?? payload;
      const root = body?.data ?? body?.result ?? body;
      const candidates = [root?.rows, root?.list, root];
      for (const candidate of candidates) {
        if (Array.isArray(candidate)) {
          return candidate;
        }
      }
      return [];
    };

    const fetchPrimaryRows = async () => {
      // NO `execType`. Measured: sending it returns `412 Invalid arguments! execType param:
      // execType`, so this route has never once succeeded. The parameter was never valid
      // here; without it the same request returns a full page.
      const params = omitUndefined({
        symbol: apiSymbol,
        currency: apiSymbol.endsWith('USDC') ? 'USDC' : 'USDT',
        offset: 0,
        limit: maxLimit,
      });
      const response = await this.xhr.get(ENDPOINTS.USER_TRADES, { params });
      return extractRows(response?.data);
    };

    const fetchLegacyRows = async () => {
      const params = omitUndefined({
        symbol: apiSymbol,
        start: fromMilliseconds,
        end: toMilliseconds,
        offset: 0,
        limit: maxLimit,
        withCount: true,
      });
      const response = await this.xhr.get(ENDPOINTS.USER_TRADES_LEGACY, { params });
      return extractRows(response?.data);
    };

    try {
      const primaryRows = await fetchPrimaryRows();
      const fallbackRows = primaryRows.length > 0 ? [] : await fetchLegacyRows();
      const deduped = new Map<string, Execution>();

      for (const execution of [...parseRows(primaryRows), ...parseRows(fallbackRows)]) {
        if (fromMilliseconds && execution.timestamp * 1000 < fromMilliseconds) {
          continue;
        }
        if (toMilliseconds && execution.timestamp * 1000 > toMilliseconds) {
          continue;
        }
        const key = `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
        deduped.set(key, execution);
      }

      return [...deduped.values()].sort((a, b) => b.timestamp - a.timestamp).slice(0, maxLimit);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  timeframes = {
    '1m': '60',
    '3m': '180',
    '5m': '300',
    '15m': '900',
    '30m': '1800',
    '1h': '3600',
    '2h': '7200',
    '3h': '10800',
    '4h': '14400',
    '6h': '21600',
    '12h': '43200',
    '1d': '86400',
    '1w': '604800',
    '1M': '2592000',
    '3M': '7776000',
    '1Y': '31104000',
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    try {
      const interval = opts.interval;
      const resolution = this.timeframes[interval];
      const limit = Math.min(opts.limit || 500, 1000);
      const [, amount, unit] = opts.interval.split(/(\d+)/);

      const end = opts.to
        ? +resolution >= 86400
          ? dayjs((dayjs(opts.to).utcOffset(0).startOf('date').unix() - 86400) * 1000)
          : dayjs(opts.to)
        : dayjs();
      const start =
        !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

      const {
        data: {
          data: { rows },
        },
      } = await this.xhr.get<{ data: { rows: any[][] } }>(ENDPOINTS.KLINE, {
        params: {
          symbol: this.formatSymbolOut(opts.symbol),
          resolution,
          from: Math.floor(start.valueOf() / 1000),
          to: Math.ceil(end.valueOf() / 1000),
          // ...(opts.limit
          //   ? {
          //       limit,
          //     }
          //   : {}),
        },
      });

      const candles: Candle[] = rows.map(([time, , , open, high, low, close, volume]) => {
        return {
          timestamp: time,
          open: parseFloat(open),
          high: parseFloat(high),
          low: parseFloat(low),
          close: parseFloat(close),
          volume: parseFloat(volume),
        };
      });

      return candles;
    } catch (err: any) {
      this.emitter.emit('error', err?.response?.data?.msg || err.message);
      return [];
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV({ ...opts, symbol: this.formatSymbolOut(opts.symbol) }, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(this.formatSymbolOut(symbol), callback);
  };

  changePositionMode = async (hedged: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }
    try {
      // PUT /g-positions/switch-pos-mode-sync?symbol=<symbol>&targetPosMode=<targetPosMode>
      const { data } = await this.xhr.put(ENDPOINTS.SET_POSITION_MODE, undefined, {
        params: {
          symbol: this.formatSymbolOut(symbol),
          targetPosMode: hedged ? 'Hedged' : 'OneWay',
        },
      });
      // {
      //   "code": 39201,
      //   "msg": "the original positions are not closed when switch pos mode",
      //   "data": null
      // }
      if (data.code !== 0) {
        this.emitAndLogError(data.msg);
        throw new Error(data.msg);
      }

      // need to filter out prev positions as mode changed, so different count
      const symbolPos = await this.fetchPositions([symbol]);
      this.store.update({
        positions: [...this.store.positions.filter((p) => p.symbol !== symbol), ...symbolPos],
      });

      const context = await this.resolveLeverageContext(symbol);
      if (context) {
        const existing = this.store.fetchedLeverage[symbol];
        this.store.update({
          fetchedLeverage: {
            ...this.store.fetchedLeverage,
            [symbol]: {
              buyLeverage: existing?.buyLeverage ?? context.longLeverage,
              sellLeverage: existing?.sellLeverage ?? context.shortLeverage,
              marginType: context.marginType,
              positionMode: context.positionMode,
            },
          },
        });
      }
    } catch (e: any) {
      this.emitAndLogError(e, e?.response?.data?.msg || e?.message);
      throw e;
    }
  };

  // Phemex has no margin-mode endpoint — the SIGN of leverageRr IS the margin
  // mode — so a leverage change must carry the current margin mode forward or
  // it silently flips the account, and a margin-type change must carry the
  // current leverage forward for the same reason.
  //
  // Both used to read that state via getOrFetchPositionsForSymbol, which is
  // wrong twice over: it early-returns [] whenever store.loaded.positions is
  // false (the hosted-mode mirror's permanent state), and mapPosition drops
  // size-0 rows on purpose, so a FLAT symbol has no store position even on a
  // fully loaded worker. Both callers then threw "Position not found" before
  // issuing any request — the margin switch never happened, and setLeverage
  // fell through to a positive leverageRr that flipped cross accounts to
  // isolated. accountPositions returns the current row with leverageRr and
  // crossMargin populated even while flat, so prefer it over possibly stale
  // symbol-scoped store rows.
  private resolveLeverageContext = async (
    symbol: string,
  ): Promise<
    { marginType: MarginType; positionMode: PositionMode; longLeverage: number; shortLeverage: number } | undefined
  > => {
    const readStoredContext = () => {
      const stored = this.store.positions.filter((p) => p.symbol === symbol);
      const storedMarginType = stored[0]?.marginType;
      const storedPositionMode = stored[0]?.positionMode;
      // Use the store only as a fallback and only when it carries both fields —
      // guessing a default here is what flips margin mode by accident.
      if (storedMarginType === undefined || storedPositionMode === undefined) return undefined;

      const long = stored.find((p) => p.side === PositionSide.Long) ?? stored[0];
      const short = stored.find((p) => p.side === PositionSide.Short) ?? stored[0];
      return {
        marginType: storedMarginType,
        positionMode: storedPositionMode,
        longLeverage: Math.abs(long.leverage),
        shortLeverage: Math.abs(short.leverage),
      };
    };

    try {
      const {
        data: { data },
      } = await this.xhr.get<{ data: POSITIONS_RESPONSE }>(ENDPOINTS.POSITIONS, {
        params: {
          currency: symbol?.endsWith('USDC') ? 'USDC' : 'USDT',
          symbol: this.formatSymbolOut(symbol),
        },
      });

      const rows = (data?.positions || []).filter((r: any) => this.formatSymbolIn(r.symbol) === symbol);
      if (rows.length === 0) return readStoredContext();

      const leverageOf = (row: any) => Math.abs(parseFloat(v(row, 'leverageRr'))) || 0;
      const long = rows.find((r: any) => v(r, 'posSide') === 'Long') ?? rows[0];
      const short = rows.find((r: any) => v(r, 'posSide') === 'Short') ?? rows[0];
      const rawLeverage = parseFloat(v(long, 'leverageRr'));

      return {
        marginType: rawLeverage < 0 || v(long, 'crossMargin') ? MarginType.Cross : MarginType.Isolated,
        positionMode: v(long, 'posMode') === 'Hedged' ? PositionMode.Hedged : PositionMode.OneWay,
        longLeverage: leverageOf(long),
        shortLeverage: leverageOf(short),
      };
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return readStoredContext();
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }

    const context = await this.resolveLeverageContext(symbol);

    if (!context) {
      const msg = `Could not read current leverage settings for ${symbol} — margin type unchanged`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    let inputLeverage = context.longLeverage;
    if (!isolated) {
      inputLeverage = -inputLeverage;
    }
    await this.setLeverageHelper(symbol, inputLeverage, undefined, true);
  };

  setLeverage = async (symbol: string, leverage: number, side?: PositionSide) => {
    return this.setLeverageHelper(symbol, leverage, side);
  };

  setLeverageHelper = async (
    symbol: string,
    inputLeverage: number,
    side?: PositionSide,
    allowChangingMarginType = false,
  ) => {
    const market = this.store.marketsBySymbol[symbol];
    const context = await this.resolveLeverageContext(symbol);

    if (!market) {
      const msg = `Market ${symbol} not found`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }
    if (!context) {
      const msg = `Could not read current leverage settings for ${symbol} — leverage unchanged`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    const prepLeverage = (l: number): number => {
      const safeLev = Math.min(Math.max(Math.abs(l), market.limits.leverage.min), market.limits.leverage.max);
      if (!allowChangingMarginType) {
        // Carry the existing margin mode forward — a bare positive value here
        // silently converts a cross account to isolated.
        if (context.marginType === MarginType.Cross) {
          return safeLev * -1;
        } else {
          return safeLev;
        }
      } else {
        if (inputLeverage < 0) {
          return safeLev * -1;
        } else {
          return safeLev;
        }
      }
    };

    let params: Record<any, any> = {
      symbol: this.formatSymbolOut(symbol),
    };

    if (context.positionMode === PositionMode.Hedged) {
      if (side) {
        if (side === PositionSide.Long) {
          params.longLeverageRr = prepLeverage(inputLeverage);
          params.shortLeverageRr = prepLeverage(context.shortLeverage || inputLeverage);
        } else {
          params.shortLeverageRr = prepLeverage(inputLeverage);
          params.longLeverageRr = prepLeverage(context.longLeverage || inputLeverage);
        }
      } else {
        params.longLeverageRr = prepLeverage(inputLeverage);
        params.shortLeverageRr = prepLeverage(inputLeverage);
      }
    } else {
      params.leverageRr = prepLeverage(inputLeverage);
    }

    const response = await this.xhr.put(ENDPOINTS.SET_LEVERAGE, undefined, {
      params,
    });

    if (response?.data?.code !== 0) {
      this.emitAndLogError(response?.data?.msg);
      throw new Error(response?.data?.msg);
    }

    const positions = await this.fetchPositions([symbol]);
    this.store.addOrUpdatePositions(positions);

    // Record the new leverage even when the symbol is flat. mapPosition drops
    // size-0 rows, so a leverage change on a symbol with no open position
    // leaves nothing in store.positions for the place-order % slider to read,
    // and it falls back to 1x — the order size then comes out `leverage` times
    // too small. fetchedLeverage is the app's documented no-position fallback
    // (see blofin's fetchLeverageForSymbol).
    const appliedSignedLeverage = prepLeverage(inputLeverage);
    const appliedLeverage = Math.abs(appliedSignedLeverage);
    const existing = this.store.fetchedLeverage[symbol];
    this.store.update({
      fetchedLeverage: {
        ...this.store.fetchedLeverage,
        [symbol]: {
          buyLeverage: side === PositionSide.Short ? (existing?.buyLeverage ?? appliedLeverage) : appliedLeverage,
          sellLeverage: side === PositionSide.Long ? (existing?.sellLeverage ?? appliedLeverage) : appliedLeverage,
          marginType: appliedSignedLeverage < 0 ? MarginType.Cross : MarginType.Isolated,
          positionMode: context.positionMode,
        },
      },
    });
  };

  cancelOrders = async (_orders: Order[]) => {
    // replace child orders with parent and remove dupes introduced
    const orders = uniqBy(
      _orders.map((o) => ({
        ...o,
        id: o.id.split('__')[0],
      })),
      'id',
    );

    try {
      const ordersWithPosSide = await mapSeries(orders, async (o) => {
        const posSide = await this.getOrderPositionSide(o);
        return {
          ...o,
          posSide,
        };
      });
      const groupedBySymbol = groupBy(ordersWithPosSide, (o) => `${o.posSide}:${o.symbol}`);
      const requests = Object.values(groupedBySymbol).map((symbolOrders) => {
        return {
          symbol: this.formatSymbolOut(symbolOrders[0].symbol),
          posSide: symbolOrders[0].posSide,
          orderIDs: symbolOrders.map((o) => o.id),
        };
      });

      const res = (
        await mapSeries(requests, async (request) => {
          if (request.orderIDs.length === 1) {
            return await this.unlimitedXHR.delete(ENDPOINTS.CANCEL_ORDER, {
              params: {
                symbol: this.formatSymbolOut(request.symbol),
                posSide: request.posSide,
                orderID: request.orderIDs[0],
              },
            });
          } else {
            const lots = chunk(request.orderIDs, 10);
            return (
              await mapSeries(lots, async (lot) => {
                return await this.unlimitedXHR.delete(ENDPOINTS.BATCH_ORDERS, {
                  params: {
                    symbol: this.formatSymbolOut(request.symbol),
                    posSide: request.posSide,
                    orderID: lot.join(','),
                  },
                });
              })
            ).flat();
          }
        })
      ).flat();
      for (const r of res) {
        if (r.data?.code !== 0) {
          throw new Error(r.data?.msg);
        }
      }
      this.store.updateOrdersStatus(
        _orders.map((order) => ({ id: order.id })),
        OrderStatus.Canceled,
      );
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    const orders = this.store.orders.filter((o) => o.symbol === symbol);
    await this.cancelOrders(orders);
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    let updatePrice;
    let updateAmount;
    if ('price' in update) updatePrice = update.price;
    if ('amount' in update) updateAmount = update.amount;
    const newOrder: Writable<PlaceOrderOpts> = {
      ...order,
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      amount: updateAmount ?? order.amount,
      price: updatePrice ?? order.price,
      reduceOnly: order.reduceOnly || false,
    };

    const realId = order.id.split('__')[0];
    const relativeOrders = this.store.orders.filter((o) => o.id.startsWith(realId));

    if (relativeOrders.length > 1) {
      const parent = relativeOrders.find((o) => !o.id.includes('__'));
      const stoploss = relativeOrders.find((o) => o.id.endsWith('__stop_loss'));
      const takeprofit = relativeOrders.find((o) => o.id.endsWith('__take_profit'));
      if (parent) {
        newOrder.type = parent.type;
        newOrder.side = parent.side;
        newOrder.amount = parent.amount;
        newOrder.price = parent.price;
        newOrder.reduceOnly = parent.reduceOnly;
      }
      if (stoploss?.price) newOrder.stopLoss = stoploss.price;
      if (stoploss?.slTriggerType) newOrder.slTriggerType = stoploss.slTriggerType;
      if (takeprofit?.price) newOrder.takeProfit = takeprofit.price;
      if (takeprofit?.tpTriggerType) newOrder.tpTriggerType = takeprofit.tpTriggerType;
      if (updatePrice !== undefined) {
        if (order.id.endsWith('__stop_loss')) newOrder.stopLoss = updatePrice;
        else if (order.id.endsWith('__take_profit')) newOrder.takeProfit = updatePrice;
        else newOrder.price = updatePrice;
      }
    }

    await this.cancelOrders(relativeOrders);
    return await this.placeOrder(newOrder);
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    const payloads = await this.formatCreateOrder(opts);
    const orderIds = [];
    const openOrders: Order[] = [];
    let errMsg = '';
    for (const payload of payloads) {
      const { data } = await this.unlimitedXHR.put(ENDPOINTS.ORDER, undefined, {
        params: payload,
      });
      if (data.code !== 0) {
        this.emitAndLogError(data.msg);
        errMsg = data.msg;
      } else if (data.data?.ordStatus === 'Suspended' && data.data?.execStatus === 'PendingAllocate') {
        errMsg = 'Order is suspended. Likely insufficient margin.';
      } else {
        orderIds.push(data.data.orderID);
        openOrders.push(...this.mapOrderUpdate(data.data).filter((order) => order.status === OrderStatus.Open));
      }
    }
    if (openOrders.length > 0) {
      this.store.addOrUpdateOrders(openOrders);
    }
    if (errMsg) {
      this.emitAndLogError(errMsg);
      throw new Error(errMsg);
    }
    return orderIds;
  };

  private getOrFetchMarketForSymbol = async (symbol: string): Promise<Market | undefined> => {
    const cachedMarket = this.store.marketsBySymbol[symbol];
    if (cachedMarket) return cachedMarket;

    const markets = await this.fetchMarkets();
    if (!this.isDisposed && Array.isArray(markets) && markets.length > 0) {
      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });
    }

    return this.store.marketsBySymbol[symbol] || markets.find((market) => market.symbol === symbol);
  };

  private formatCreateOrder = async (opts: PlaceOrderOpts) => {
    if (opts.type === OrderType.TrailingStopLoss) {
      return await this.formatCreateTrailingStopLossOrder(opts);
    }

    if (opts.type === OrderType.Conditional) {
      const message =
        'Phemex opening trigger orders are disabled until stopPxRp and triggerType are implemented and verified.';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const market = await this.getOrFetchMarketForSymbol(opts.symbol);

    if (!market) {
      const msg = `Market ${opts.symbol} not found`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

    const pSide = await this.getOrderPositionSide(opts);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);

    // We use price only for limit orders
    // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
    const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;

    const priceField = isStopOrTP ? 'stopPxRp' : 'priceRp';

    const positionsForSymbol = await this.getOrFetchPositionsForSymbol(opts.symbol);
    const position = positionsForSymbol[0];

    const reduceOnly = position?.positionMode === PositionMode.OneWay && opts.reduceOnly;
    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    const req = omitUndefined({
      clOrdID: `${BROKER_ID}_${uuid()}`,
      symbol: this.formatSymbolOut(opts.symbol),
      posSide: pSide,
      side: capitalize(inverseObj(ORDER_SIDE)[opts.side]),
      // ordType:
      // opts.type === OrderType.TakeProfit
      //   ? 'MarketIfTouched'
      //   : opts.type === OrderType.StopLoss
      //     ? 'Stop'
      //     : capitalize(inverseObj(ORDER_TYPE)[opts.type]),
      ordType: ORDER_TYPE[opts.type],
      orderQtyRq: amount ? `${amount}` : undefined,
      [priceField]: price ? `${price}` : undefined,
      priceRp: price ? `${price}` : undefined,
      timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
      closeOnTrigger: isStopOrTP ? 'true' : undefined,
      reduceOnly: reduceOnly && !isStopOrTP ? 'true' : undefined,
      stopLossRp: opts.stopLoss ? `${stopLoss}` : undefined,
      takeProfitRp: opts.takeProfit ? `${takeProfit}` : undefined,
      ...(opts.takeProfit
        ? { tpTrigger: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'ByMarkPrice' : 'ByLastPrice' }
        : {}),
      ...(opts.stopLoss
        ? { slTrigger: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'ByLastPrice' : 'ByMarkPrice' }
        : {}),
      ...(opts.type === OrderType.TakeProfit && {
        triggerType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'ByMarkPrice' : 'ByLastPrice',
      }),
      ...(opts.type === OrderType.StopLoss && {
        triggerType: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'ByLastPrice' : 'ByMarkPrice',
      }),
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);

    const payloads: Array<Record<string, any>> = times(lots, () => ({
      ...req,
      orderQtyRq: lotSize ? `${lotSize}` : undefined,
    }));

    if (rest) {
      payloads.push({ ...req, orderQtyRq: `${rest}` });
    }

    // if (opts.stopLoss) {
    //   payloads.push({
    //     ...omit(req, 'price'),
    //     side: capitalize(inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy]),
    //     //type: inverseObj(ORDER_TYPE)[OrderType.StopLoss],
    //     ordType: 'Stop',
    //     stopPxRp: `${opts.stopLoss}`,
    //     timeInForce: 'GoodTillCancel',
    //     closeOnTrigger: 'true',
    //     triggerType: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'ByLastPrice' : 'ByMarkPrice',
    //   });
    // }

    // if (opts.takeProfit) {
    //   payloads.push({
    //     ...omit(req, 'price'),
    //     side: capitalize(inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy]),
    //     ordType: 'MarketIfTouched',
    //     stopPxRp: `${opts.takeProfit}`,
    //     timeInForce: 'GoodTillCancel',
    //     closeOnTrigger: 'true',
    //     triggerType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'ByMarkPrice' : 'ByLastPrice',
    //   });
    // }

    // A unique ID per payload, otherwise the venue duplicates the IDs in batches.
    const clientOrderId = this.resolveCallerClientOrderId(opts);
    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
    for (const payload of payloads) {
      payload.clOrdID = clientOrderId ?? `${BROKER_ID}_${uuid()}`;
    }

    return payloads;
  };

  private formatCreateTrailingStopLossOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickersBySymbol[opts.symbol];

    const pSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;

    const position = this.store.positions.find((p) => p.symbol === opts.symbol && p.side === pSide);

    if (!market) {
      const msg = `Market ${opts.symbol} not found`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }
    if (!ticker) {
      const msg = `Ticker ${opts.symbol} not found`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }
    if (!position) {
      const msg = `No ${pSide} position open for ${opts.symbol} — nothing to attach a trailing stop to`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    const priceDistance = adjust(
      Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
      market.precision.price,
    );

    //const distancePercentage = Math.round(((priceDistance * 100) / ticker.last) * 10) / 10;

    // This formatter remains capability-gated by supportsTrailingStopLoss=false.
    // Keep its dormant wire shape aligned with PUT /g-orders/create so a future
    // enablement cannot silently submit a quantity-less order.
    const payload = {
      symbol: this.formatSymbolOut(opts.symbol),
      posSide: await this.getOrderPositionSide(opts),
      side: capitalize(inverseObj(ORDER_SIDE)[opts.side]),
      orderQtyRq: `${position.contracts}`,
      ordType: 'Stop',
      //triggerType: 'ByMarkPrice',
      pegOffsetValueRp: `${opts.side === OrderSide.Sell ? -1 * priceDistance : priceDistance}`,
      pegPriceType: 'TrailingStopPeg',
      //callbackRate: `${distancePercentage}`,
      //priceProtect: 'true',
      clOrdID: `${BROKER_ID}_${uuid()}`,
    };

    return [payload];
  };

  private getOrderPositionSide = async (
    opts: Pick<PlaceOrderOpts, 'side' | 'type' | 'reduceOnly' | 'symbol'> & Pick<Order, 'info'>,
  ) => {
    if (opts.info?.posSide) return opts.info.posSide;
    let positionSide = 'Merged';

    const positions = await this.getOrFetchPositionsForSymbol(opts.symbol);
    let isOneway = positions.some((p) => p.positionMode === PositionMode.OneWay);

    if (!positions.length) {
      isOneway = (await this.fetchRawPositionModeForSymbol(opts.symbol)) === PositionMode.OneWay;
    }

    // We need to specify side of the position to interract with
    // if we are in hedged mode on the bitmex account
    if (!isOneway) {
      positionSide = opts.side === OrderSide.Buy ? 'Long' : 'Short';

      if (
        opts.type === OrderType.StopLoss ||
        opts.type === OrderType.TakeProfit ||
        opts.type === OrderType.TrailingStopLoss ||
        opts.reduceOnly
      ) {
        positionSide = positionSide === 'Long' ? 'Short' : 'Long';
      }
    }

    return positionSide;
  };

  private fetchRawPositionModeForSymbol = async (symbol: string): Promise<PositionMode | undefined> => {
    try {
      const {
        data: { data },
      } = await this.xhr.get<{ data: POSITIONS_RESPONSE }>(ENDPOINTS.POSITIONS, {
        params: {
          currency: symbol?.endsWith('USDC') ? 'USDC' : 'USDT',
          symbol: this.formatSymbolOut(symbol),
        },
      });
      const rows = data?.positions || [];
      const row = rows.find((position) => this.formatSymbolIn(position.symbol) === symbol);
      if (row?.posMode === 'OneWay') return PositionMode.OneWay;
      if (row?.posMode === 'Hedged') return PositionMode.Hedged;
      return undefined;
    } catch (e: any) {
      this.emitAndLogError(e, e?.response?.data?.msg || e?.message);
      return undefined;
    }
  };

  formatSymbolIn = (symbol?: string) => symbol?.toUpperCase() || symbol || '';
  formatSymbolOut = (symbol?: string) =>
    this?.store?.markets.find((m) => m.symbol === symbol)?.info?.symbol || symbol || '';
}
