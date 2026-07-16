import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import groupBy from 'lodash/groupBy';
import isNull from 'lodash/isNull';
import omit from 'lodash/omit';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';

import { parsePrecision } from '@tealstreet/ccxt-compat/src/functions/ccxt_exchange';
import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
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
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createAPI } from './bitunix.api';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  TIME_IN_FORCE,
} from './bitunix.types';
import { BitunixFuturesWebsocket } from './bitunix.ws-futures';
import { BitunixPrivateWebsocket } from './bitunix.ws-private';
import {
  BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION,
  BitunixPublicWebsocket,
} from './bitunix.ws-public';

// TODO: translate our err messages
export class BitunixExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BitunixPublicWebsocket;
  publicWebsocketSecondary: BitunixPublicWebsocket;
  publicWebsocketShards: BitunixPublicWebsocket[] = [];
  privateWebsocket: BitunixPrivateWebsocket;
  futuresWebsocket: BitunixFuturesWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  enableFakeFullTpSl = false;

  accountPositionMode: PositionMode = PositionMode.Hedged;
  lastTickAccountPositionMode: PositionMode = PositionMode.Hedged;

  symbolSettings: Map<string, { marginType: MarginType; leverage: number }> = new Map();

  // Cache positionIds independently from the store's info field
  // This ensures positionIds survive even if position.info gets lost during store updates
  positionIdCache: Map<string, string> = new Map(); // key: "SYMBOL:side" -> positionId

  // Cache order TIFs by clientId — Bitunix REST API bug: get_pending_orders always returns effect:"GTC"
  orderTifCache: Map<string, OrderTimeInForce> = new Map();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BitunixPublicWebsocket(this, false);
    this.publicWebsocketSecondary = new BitunixPublicWebsocket(this, true);
    this.privateWebsocket = new BitunixPrivateWebsocket(this);
    this.futuresWebsocket = new BitunixFuturesWebsocket(this);
  }

  private isRecord = (value: unknown): value is Record<string, any> => {
    return !!value && typeof value === 'object' && !Array.isArray(value);
  };

  private getResponseCandidates = (payload: unknown) => {
    const candidates: unknown[] = [];
    const seen = new Set<object>();

    const visit = (value: unknown, depth = 0) => {
      if (value === undefined || value === null || depth > 4) {
        return;
      }

      if (typeof value === 'object') {
        if (seen.has(value)) {
          return;
        }
        seen.add(value);
      }

      candidates.push(value);

      if (!this.isRecord(value)) {
        return;
      }

      visit(value.data, depth + 1);
      visit(value.result, depth + 1);
      visit(value.payload, depth + 1);
      visit(value.positionList, depth + 1);
      visit(value.positions, depth + 1);
      visit(value.list, depth + 1);
      visit(value.rows, depth + 1);
    };

    visit(payload);

    return candidates;
  };

  private getFirstValue = (record: Record<string, any>, fields: string[]) => {
    for (const field of fields) {
      const value = record[field];
      if (value !== undefined && value !== null && value !== '') {
        return value;
      }
    }
    return undefined;
  };

  private parseFiniteNumber = (value: unknown, fallback = 0) => {
    if (value === undefined || value === null || value === '') {
      return fallback;
    }

    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
  };

  private hasFiniteTriggerPrice = (value: unknown) => {
    if (value === undefined || value === null || value === '') {
      return false;
    }

    return Number.isFinite(Number(value));
  };

  getTpslLegOrderId = (rawOrderId: unknown, positionId: unknown, leg: 'tp' | 'sl', disambiguate: boolean) => {
    const orderId = String(rawOrderId ?? '');
    if (!disambiguate) {
      return orderId;
    }

    const positionPart =
      positionId === undefined || positionId === null || positionId === '' ? 'position' : String(positionId);
    return `${orderId}__${positionPart}__${leg}`;
  };

  private getTpslRawOrderId = (order: Pick<Order, 'id'> & { info?: Record<string, any> }) => {
    const rawOrderId = order.info?.id ?? order.info?.orderId;
    if (rawOrderId !== undefined && rawOrderId !== null && rawOrderId !== '') {
      return String(rawOrderId);
    }

    return order.id;
  };

  private getVirtualTpslLeg = (orderId: string): 'tp' | 'sl' | undefined => {
    if (orderId.endsWith('__tp')) return 'tp';
    if (orderId.endsWith('__sl')) return 'sl';
    return undefined;
  };

  private getVirtualTpslParentOrderId = (orderId: string) => {
    const leg = this.getVirtualTpslLeg(orderId);
    if (!leg) {
      return orderId;
    }

    return orderId.slice(0, orderId.length - `__${leg}`.length);
  };

  private getAttachedVirtualTpslLeg = (order: Order): 'tp' | 'sl' | undefined => {
    const leg = this.getVirtualTpslLeg(order.id);
    if (!leg) {
      return undefined;
    }

    const parentOrderId = this.getVirtualTpslParentOrderId(order.id);
    const parentOrder = this.store.orders.find((candidate) => candidate.id === parentOrderId);

    return parentOrder && parentOrder.type !== OrderType.TakeProfit && parentOrder.type !== OrderType.StopLoss
      ? leg
      : undefined;
  };

  private extractAccountRow = (payload: unknown, marginCoin = 'USDT') => {
    const accountFields = [
      'marginCoin',
      'available',
      'availableBalance',
      'frozen',
      'margin',
      'positionMode',
      'crossUnrealizedPNL',
      'isolationUnrealizedPNL',
    ];

    const isAccountRow = (value: unknown): value is Record<string, any> => {
      return this.isRecord(value) && accountFields.some((field) => value[field] !== undefined);
    };

    for (const candidate of this.getResponseCandidates(payload)) {
      if (Array.isArray(candidate)) {
        const rows = candidate.filter(isAccountRow);
        const preferred = rows.find((row) => String(row.marginCoin || '').toUpperCase() === marginCoin.toUpperCase());
        if (preferred || rows[0]) {
          return preferred || rows[0];
        }
      }

      if (isAccountRow(candidate)) {
        return candidate;
      }
    }

    return {};
  };

  private extractPositionRows = (payload: unknown) => {
    const isPositionRow = (value: unknown): value is Record<string, any> => {
      return this.isRecord(value) && !!this.getFirstValue(value, ['symbol', 'instId', 'contract', 'instrument']);
    };

    for (const candidate of this.getResponseCandidates(payload)) {
      if (!Array.isArray(candidate)) {
        continue;
      }

      const rows = candidate.filter(isPositionRow);
      if (rows.length > 0) {
        return rows;
      }
    }

    return [];
  };

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.publicWebsocketSecondary.dispose();
    this.publicWebsocketShards.forEach((websocket) => websocket.dispose());
    this.futuresWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  private configurePublicWebsocketShards = (markets: Market[]) => {
    this.publicWebsocketShards.forEach((websocket) => websocket.dispose());
    this.publicWebsocketShards = [];

    const symbolShards = chunk(
      markets.map((market) => market.symbol),
      BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION,
    );
    const firstShard = symbolShards[0] || [];
    this.publicWebsocket.setMarketSymbols(firstShard);
    this.publicWebsocketSecondary.setMarketSymbols(firstShard);

    for (const symbols of symbolShards.slice(1)) {
      this.publicWebsocketShards.push(
        new BitunixPublicWebsocket(this, false, symbols),
        new BitunixPublicWebsocket(this, true, symbols),
      );
    }

    return [this.publicWebsocket, this.publicWebsocketSecondary, ...this.publicWebsocketShards];
  };

  validateAccount = async () => {
    try {
      const response = await this.xhr.get(ENDPOINTS.ACCOUNT, {
        params: {
          marginCoin: 'USDT',
        },
      });
      if (response.data.code !== 0) {
        return { error: response.data.msg || 'Bitunix account validation failed' };
      }
      this.extractAccountRow(response?.data);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
    return {
      error: '',
      balance: await this.fetchBalance()
        .then((r) => r.balance)
        .catch(() => undefined),
    };
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

      // start futures right away for load book
      this.futuresWebsocket.connectAndSubscribe();

      // load initial tickers data
      // then we use websocket for live data
      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Bitunix markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      // Each public connection carries one channel per market. Bitunix caps a
      // connection at 300 subscriptions, so shard both ticker and price feeds.
      this.configurePublicWebsocketShards(markets).forEach((websocket) => websocket.connectAndSubscribe());
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    // fetch current position mode (Hedge/One-way)
    this.store.setSetting('isHedged', await this.fetchPositionMode());

    // start ticking live data
    // balance, tickers, positions
    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Bitunix`);

    // fetch unfilled orders
    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded Bitunix orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });

  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        if (count % 32 === 0 || this.lastTickAccountPositionMode !== this.accountPositionMode) {
          this.lastTickAccountPositionMode = this.accountPositionMode;
          const positionsWithRisk = await this.fetchPositions();
          this.store.update({ positions: positionsWithRisk });
          const orders = await this.fetchOrders();
          this.store.update({ orders });
          if (this.isDisposed) return;
        }
        const { balance, positions } = await this.fetchBalance();
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
        loop(() => this.tick(count + 1), this.getTickInterval());
      }
    }
  };

  /*
  {
    "symbol": "GUSDT",
    "base": "G",
    "quote": "USDT",
    "minTradeVolume": "300",
    "minBuyPriceOffset": null,
    "maxSellPriceOffset": null,
    "maxLimitOrderVolume": "10000000",
    "maxMarketOrderVolume": "3000000",
    "basePrecision": 0,
    "quotePrecision": 5,
    "maxLeverage": 50,
    "minLeverage": 1,
    "defaultLeverage": 20,
    "defaultMarginMode": "2",
    "priceProtectScope": null,
    "symbolStatus": "OPEN"
  }
  */
  fetchMarkets = async () => {
    try {
      const {
        data: { data: symbols },
      } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.MARKETS);

      const markets: Market[] = symbols
        .filter(
          (m) =>
            v(m, 'symbolStatus') === 'OPEN' &&
            v(m, 'quote') === 'USDT' &&
            v(m, 'isApiSupported') !== false,
        )
        .map((m) => {
          const baseAsset = v(m, 'base');
          const quoteAsset = v(m, 'quote');

          return {
            id: `${baseAsset}/${quoteAsset}`,
            symbol: m.symbol,
            base: baseAsset,
            quote: quoteAsset,
            active: v(m, 'symbolStatus') === 'OPEN',
            precision: {
              amount: +(parsePrecision(v(m, 'basePrecision')) || 0),
              price: +(parsePrecision(v(m, 'quotePrecision')) || 0),
            },
            limits: {
              amount: {
                min: 0,
                max: v(m, 'maxLimitOrderVolume'),
              },
              leverage: {
                min: 1,
                max: 100,
              },
            },
          };
        });

      return markets;
    } catch (err: any) {
      console.trace(err);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    try {
      const {
        data: { data: tickers },
      } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.TICKERS);

      return tickers.reduce((acc: Ticker[], ticker) => {
        const market = this.store.marketsBySymbol[ticker.symbol];

        if (!market) return acc;

        //console.log('ticket', ticker);

        const last = parseFloat(v(ticker, 'last'));
        const open = parseFloat(v(ticker, 'open'));
        const finalTicker: Ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: parseFloat(v(ticker, 'last')),
          ask: parseFloat(v(ticker, 'last')),
          last,
          mark: parseFloat(v(ticker, 'markPrice')),
          index: parseFloat(v(ticker, 'markPrice')),
          percentage: open === 0 ? 0 : ((last - open) / open) * 100,
          fundingRate: 0,
          fundingTime: 0,
          // fundingRate: parseFloat(v(ticker, 'lastFundingRate')),
          // fundingTime: parseFloat(v(ticker, 'nextFundingTime')),
          volume: parseFloat(v(ticker, 'baseVol')),
          quoteVolume: parseFloat(v(ticker, 'quoteVol')),
          openInterest: 0, // Bitunix ticker endpoint doesn't provide open interest
        };

        return [...acc, finalTicker];
      }, []);
    } catch (err: any) {
      console.trace(err);
      return this.store.tickers;
    }
  };

  fetchPositions = async () => {
    try {
      const response = await this.xhr.get<{ data: Record<string, any> }>(ENDPOINTS.POSITIONS);

      // Bitunix has returned positions as direct arrays, positionList wrappers,
      // and extra nested data wrappers across endpoint revisions.
      const positionList = this.extractPositionRows(response?.data);

      const formattedPositions = positionList.reduce<Position[]>((acc, p) => {
        const symbol = String(this.getFirstValue(p, ['symbol', 'instId', 'contract', 'instrument']) || '');
        if (!symbol) {
          return acc;
        }

        const contractsRaw = this.getFirstValue(p, ['qty', 'maxQty', 'size', 'positionAmt', 'amount', 'contracts']);
        const contracts = this.parseFiniteNumber(contractsRaw);
        const positionModeStr = String(this.getFirstValue(p, ['positionMode', 'posMode']) || '').toUpperCase();
        const positionMode = positionModeStr === 'ONE_WAY' ? PositionMode.OneWay : PositionMode.Hedged;
        const marginModeStr = String(this.getFirstValue(p, ['marginMode', 'marginType']) || '').toUpperCase();
        const symbolSettings = this.symbolSettings.get(symbol);
        const marginType =
          marginModeStr === 'CROSS'
            ? MarginType.Cross
            : marginModeStr === 'ISOLATION' || marginModeStr === 'ISOLATED'
              ? MarginType.Isolated
              : symbolSettings?.marginType || MarginType.Cross;

        // Bitunix API may return side as BUY/SELL or LONG/SHORT depending on endpoint version
        const sideStr = String(this.getFirstValue(p, ['side', 'positionSide']) || '').toUpperCase();
        const isLong = sideStr === 'BUY' || sideStr === 'LONG' || (sideStr === 'BOTH' && contracts >= 0);

        const side = isLong ? PositionSide.Long : PositionSide.Short;

        // NOTE: Do NOT cache positionId from REST here. The REST API often returns
        // stale/cached data (especially via throttledFetchPositions), which would
        // overwrite the correct WS-sourced positionId. Only WS position events
        // (in bitunix.ws-private.ts) should update positionIdCache.
        // REST positionId is still available via position.info.positionId as fallback.

        const res: Position = {
          id: createPositionId(symbol, side, positionMode),
          symbol,
          side,
          entryPrice: this.parseFiniteNumber(
            this.getFirstValue(p, ['avgOpenPrice', 'entryPrice', 'openPrice', 'avgPrice']),
          ),
          notional: this.parseFiniteNumber(this.getFirstValue(p, ['entryValue', 'notional', 'positionValue'])),
          leverage: this.parseFiniteNumber(
            this.getFirstValue(p, ['leverage', 'lever']),
            symbolSettings?.leverage || 10,
          ),
          unrealizedPnl: this.parseFiniteNumber(
            this.getFirstValue(p, ['unrealizedPNL', 'unrealizedPnl', 'unRealizedProfit', 'upl']),
          ),
          realizedPnl: this.parseFiniteNumber(this.getFirstValue(p, ['realizedPNL', 'realizedPnl'])),
          contracts: Math.abs(contracts),
          liquidationPrice: this.parseFiniteNumber(this.getFirstValue(p, ['liqPrice', 'liquidationPrice', 'liqPx'])),
          marginType,
          positionMode,
          info: p,
        };
        acc.push(res);
        return acc;
      }, []);

      const createFakePosition = (m: Market, side: PositionSide, positionMode: PositionMode) => {
        const symbolSettings = this.symbolSettings.get(m.symbol);
        return {
          id: createPositionId(m.symbol, side, positionMode),
          symbol: m.symbol,
          side,
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          leverage: symbolSettings?.leverage || 10,
          liquidationPrice: 0,
          positionMode,
          marginType: symbolSettings?.marginType || MarginType.Cross,
        };
      };

      const mode = this.accountPositionMode;
      const fakePositions = this.store.markets.reduce((acc: Position[], m) => {
        acc.push(...[createFakePosition(m, PositionSide.Long, mode), createFakePosition(m, PositionSide.Short, mode)]);
        return acc;
      }, []);

      return uniqBy([...formattedPositions, ...fakePositions], 'id');
    } catch (err: any) {
      console.error('[Bitunix] fetchPositions failed:', err?.message || err);
      return this.store.positions;
    }
  };

  fetchBalance = async () => {
    try {
      const response = await this.xhr.get<{ data: Record<string, any> }>(ENDPOINTS.ACCOUNT, {
        params: {
          marginCoin: 'USDT',
        },
      });

      const account = this.extractAccountRow(response?.data);
      const positionModeStr = String(this.getFirstValue(account, ['positionMode', 'posMode']) || '').toUpperCase();
      this.accountPositionMode = positionModeStr === 'ONE_WAY' ? PositionMode.OneWay : PositionMode.Hedged;

      const free = this.parseFiniteNumber(
        this.getFirstValue(account, ['available', 'availableBalance', 'availableMargin']),
      );
      const crossUnrealizedPnl = this.getFirstValue(account, ['crossUnrealizedPNL', 'crossUnrealizedPnl']);
      const isolationUnrealizedPnl = this.getFirstValue(account, ['isolationUnrealizedPNL', 'isolationUnrealizedPnl']);
      const upnl =
        crossUnrealizedPnl !== undefined || isolationUnrealizedPnl !== undefined
          ? this.parseFiniteNumber(crossUnrealizedPnl) + this.parseFiniteNumber(isolationUnrealizedPnl)
          : this.parseFiniteNumber(this.getFirstValue(account, ['unrealizedPNL', 'unrealizedPnl', 'upnl']));
      const used = this.parseFiniteNumber(this.getFirstValue(account, ['margin', 'used', 'usedMargin']));
      const total = free + used + this.parseFiniteNumber(this.getFirstValue(account, ['frozen', 'locked']));

      const balance = { total, free, used, upnl };

      return { balance, positions: this.store.positions || [] };
    } catch (err: any) {
      console.trace(err);
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { balance: this.store.balance, positions: this.store.positions };
    }
  };

  fetchOrders = async () => {
    const [regularOrdersResult, tpslOrdersResult] = await Promise.allSettled([
      this.fetchRegularOrders(),
      this.fetchTpslOrders(),
    ]);

    const regularOrders = regularOrdersResult.status === 'fulfilled' ? regularOrdersResult.value : [];
    const tpslOrders = tpslOrdersResult.status === 'fulfilled' ? tpslOrdersResult.value : [];

    return [...regularOrders, ...tpslOrders];
  };

  fetchRegularOrders = async () => {
    try {
      const response = await this.xhr.get<{
        data?: { orderList?: Array<Record<string, any>> } | Array<Record<string, any>>;
      }>(ENDPOINTS.OPEN_ORDERS, {
        __suppressExchangeError: true,
        __suppressExchangeRateLimitError: true,
      } as any);
      const data = response?.data?.data;
      const orderList: Array<Record<string, any>> = Array.isArray(data)
        ? data
        : Array.isArray(data?.orderList)
          ? data.orderList
          : [];

      const orders: Order[] = orderList.flatMap((o) => {
        // Bitunix REST API bug: get_pending_orders always returns effect:"GTC"
        // even for POST_ONLY orders. Use cached TIF from order placement or WS event.
        const cachedTif = this.orderTifCache.get(o.clientId);
        const restTif = TIME_IN_FORCE[o.effect?.toUpperCase()] || OrderTimeInForce.GoodTillCancel;
        const timeInForce = cachedTif || restTif;

        const baseOrder = {
          id: v(o, 'orderId'),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: ORDER_TYPE[o.orderType?.toUpperCase()],
          side: ORDER_SIDE[o.side],
          price: parseFloat(o.price), // || parseFloat(v(o, 'stopPrice')),
          amount: parseFloat(v(o, 'qty')),
          reduceOnly: v(o, 'reduceOnly') || false,
          filled: parseFloat(v(o, 'tradeQty')),
          lastUpdate: v(o, 'mtime'),
          remaining: subtract(v(o, 'qty'), v(o, 'tradeQty')),
          takeProfitPrice: parseFloat(v(o, 'tpOrderPrice')),
          stopLossPrice: parseFloat(v(o, 'slOrderPrice')),
          timeInForce,
          info: o,
          // Bitunix omits stop type when it's MarkPrice (the default)
          tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'tpStopType')] || OrderStopTriggerType.MarkPrice,
          slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'slStopType')] || OrderStopTriggerType.MarkPrice,
        };

        const subList = [baseOrder];

        // Create virtual TP/SL orders if they exist on the limit order
        if (!isNull(o.tpPrice)) {
          subList.push({
            id: `${v(o, 'orderId')}__tp`,
            status: OrderStatus.Open,
            symbol: o.symbol,
            type: OrderType.TakeProfit,
            side: ORDER_SIDE[o.side] === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
            price: parseFloat(o.tpPrice),
            amount: parseFloat(v(o, 'qty')),
            reduceOnly: true,
            filled: 0,
            lastUpdate: v(o, 'mtime'),
            remaining: parseFloat(v(o, 'qty')),
            takeProfitPrice: 0,
            stopLossPrice: 0,
            timeInForce: OrderTimeInForce.GoodTillCancel,
            info: o,
            tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'tpStopType')] || OrderStopTriggerType.MarkPrice,
            slTriggerType: OrderStopTriggerType.MarkPrice,
          });
        }

        if (!isNull(o.slPrice)) {
          subList.push({
            id: `${v(o, 'orderId')}__sl`,
            status: OrderStatus.Open,
            symbol: o.symbol,
            type: OrderType.StopLoss,
            side: ORDER_SIDE[o.side] === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
            price: parseFloat(o.slPrice),
            amount: parseFloat(v(o, 'qty')),
            reduceOnly: true,
            filled: 0,
            lastUpdate: v(o, 'mtime'),
            remaining: parseFloat(v(o, 'qty')),
            takeProfitPrice: 0,
            stopLossPrice: 0,
            timeInForce: OrderTimeInForce.GoodTillCancel,
            info: o,
            tpTriggerType: OrderStopTriggerType.MarkPrice,
            slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'slStopType')] || OrderStopTriggerType.MarkPrice,
          });
        }

        return subList;
      });

      return orders;
    } catch (err: any) {
      this.log(`fetchRegularOrders failed: ${err?.response?.data?.msg || err?.message}`, LogSeverity.Warning);
      return [];
    }
  };

  /*
    [
      {
          "id": "5756810139757749362",
          "positionId": "6935449161279779517",
          "symbol": "BTCUSDT",
          "base": "BTC",
          "quote": "USDT",
          "tpPrice": null,
          "tpStopType": "LAST_PRICE",
          "slPrice": "86782.5",
          "slStopType": "LAST_PRICE",
          "tpOrderType": null,
          "tpOrderPrice": null,
          "slOrderType": "MARKET",
          "slOrderPrice": null,
          "tpQty": null,
          "slQty": "0.0007"
      },
      {
          "id": "1160227842354192817",
          "positionId": "6935449161279779517",
          "symbol": "BTCUSDT",
          "base": "BTC",
          "quote": "USDT",
          "tpPrice": "79176.3",
          "tpStopType": "LAST_PRICE",
          "slPrice": null,
          "slStopType": "LAST_PRICE",
          "tpOrderType": "MARKET",
          "tpOrderPrice": null,
          "slOrderType": null,
          "slOrderPrice": null,
          "tpQty": "0.0007",
          "slQty": null
      },
      {
          "id": "7414023499489540835",
          "positionId": "6935449161279779517",
          "symbol": "BTCUSDT",
          "base": "BTC",
          "quote": "USDT",
          "tpPrice": "77850.7",
          "tpStopType": "LAST_PRICE",
          "slPrice": "87319.1",
          "slStopType": "LAST_PRICE",
          "tpOrderType": null,
          "tpOrderPrice": null,
          "slOrderType": "MARKET",
          "slOrderPrice": null,
          "tpQty": "0.001",
          "slQty": "0.001"
      }
    ]
  */
  fetchTpslOrders = async () => {
    const response = await this.xhr.get<{ data: any }>(ENDPOINTS.OPEN_TPSL_ORDERS, {
      __suppressExchangeError: true,
      __suppressExchangeRateLimitError: true,
    } as any);
    const data = response?.data?.data;

    // Bitunix API may return TPSL orders as a direct array or wrapped in a list property
    const orderList: Array<Record<string, any>> = Array.isArray(data)
      ? data
      : Array.isArray(data?.orderList)
        ? data.orderList
        : [];

    if (!orderList || orderList.length === 0) return [];

    const positionsLookup = new Map<string, Position>();
    this.store.positions.forEach((p) => {
      positionsLookup.set(p.info?.positionId, p);
    });

    const orders: Order[] = orderList.flatMap((o) => {
      const subList = [];
      const position = positionsLookup.get(o.positionId);
      const rawOrderId = this.getTpslRawOrderId({ id: String(v(o, 'id') || v(o, 'orderId') || ''), info: o });
      const hasTp = this.hasFiniteTriggerPrice(o.tpPrice);
      const hasSl = this.hasFiniteTriggerPrice(o.slPrice);
      const useLegOrderIds = hasTp && hasSl;
      const orderInfo = {
        ...o,
        id: rawOrderId,
        orderId: rawOrderId,
      };

      if (hasTp) {
        const tpQty = this.parseFiniteNumber(v(o, 'tpQty'));
        const isFullTp = o.positionId && position && +position.contracts === tpQty;
        subList.push({
          id: this.getTpslLegOrderId(rawOrderId, o.positionId, 'tp', useLegOrderIds),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: OrderType.TakeProfit,
          side: position ? (position?.side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy) : ORDER_SIDE[o.side],
          price: parseFloat(o.tpPrice), // || parseFloat(v(o, 'stopPrice')),
          amount: this.enableFakeFullTpSl && isFullTp ? 0 : tpQty,
          reduceOnly: v(o, 'reduceOnly') || false,
          filled: 0,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: tpQty,
          info: orderInfo,
          tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'tpStopType')] || OrderStopTriggerType.MarkPrice,
        });
      }

      if (hasSl) {
        const slQty = this.parseFiniteNumber(v(o, 'slQty'));
        const isFullSl = o.positionId && position && +position.contracts === slQty;
        subList.push({
          id: this.getTpslLegOrderId(rawOrderId, o.positionId, 'sl', useLegOrderIds),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: OrderType.StopLoss,
          side: position ? (position?.side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy) : ORDER_SIDE[o.side],
          price: parseFloat(o.slPrice), // || parseFloat(v(o, 'stopPrice')),
          amount: this.enableFakeFullTpSl && isFullSl ? 0 : slQty,
          reduceOnly: v(o, 'reduceOnly') || false,
          filled: 0,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: slQty,
          info: orderInfo,
          slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'slStopType')] || OrderStopTriggerType.MarkPrice,
        });
      }

      return subList;
    });

    return orders;
  };

  fetchExecutions = async (opts: ExecutionsOptions): Promise<Execution[]> => {
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

    const maxLimit = Math.min(Math.max(limit, 1), 100);

    const normalizeSymbolKey = (value: unknown): string =>
      String(value ?? '')
        .toUpperCase()
        .replace(/[^A-Z0-9]/g, '');

    const normalizeToMilliseconds = (value?: number) => {
      if (value === undefined || value === null || !Number.isFinite(value)) {
        return undefined;
      }
      const normalized = Math.floor(value);
      if (normalized >= 1_000_000_000_000_000_000) {
        return Math.floor(normalized / 1_000_000);
      }
      if (normalized >= 1_000_000_000_000_000) {
        return Math.floor(normalized / 1_000);
      }
      if (normalized >= 1_000_000_000_000) {
        return normalized;
      }
      return normalized * 1000;
    };

    const normalizeToSeconds = (value: unknown) => {
      if (value === undefined || value === null) {
        return undefined;
      }

      const numeric = Number(value);
      if (Number.isFinite(numeric) && numeric > 0) {
        const normalized = Math.floor(numeric);
        if (normalized >= 1_000_000_000_000_000_000) {
          return Math.floor(normalized / 1_000_000_000);
        }
        if (normalized >= 1_000_000_000_000_000) {
          return Math.floor(normalized / 1_000_000);
        }
        if (normalized >= 1_000_000_000_000) {
          return Math.floor(normalized / 1000);
        }
        return normalized;
      }

      const parsedDate = Date.parse(String(value));
      if (Number.isFinite(parsedDate) && parsedDate > 0) {
        return Math.floor(parsedDate / 1000);
      }

      return undefined;
    };

    const fromMilliseconds = normalizeToMilliseconds(from);
    const toMilliseconds = normalizeToMilliseconds(to);
    const marketSymbolKey = normalizeSymbolKey(market.symbol);

    const parseRows = (rows: Array<Record<string, any>>) =>
      rows.reduce<Execution[]>((acc, row) => {
        const rowSymbol = String(row.symbol || row.instId || row.contract || row.instrument || market.symbol || '');
        if (normalizeSymbolKey(rowSymbol) !== marketSymbolKey) {
          return acc;
        }

        const side = ORDER_SIDE[String(row.side || '').toUpperCase()];
        if (!side) {
          return acc;
        }

        const price = parseFloat(row.price || row.avgPrice || row.fillPrice || '0');
        const qty = parseFloat(row.qty || row.tradeQty || row.fillSize || '0');
        if (!Number.isFinite(price) || !Number.isFinite(qty) || price <= 0 || qty <= 0) {
          return acc;
        }

        const timestamp = normalizeToSeconds(
          row.ctime || row.mtime || row.time || row.ts || row.tradeTime || row.createTime || row.updatedTime,
        );
        if (!timestamp) {
          return acc;
        }

        const executionOrderId = String(row.orderId || row.ordId || '');
        if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
          return acc;
        }

        const fee = parseFloat(row.fee || row.execFee || '0');
        acc.push(
          omitUndefined({
            timestamp,
            symbol: market.symbol,
            orderId: executionOrderId || undefined,
            price,
            qty,
            side,
            commission: Number.isFinite(fee) ? Math.abs(fee) : undefined,
            commissionAsset: row.feeCurrency || row.feeCoin || row.marginCoin || market.quote,
          }),
        );
        return acc;
      }, []);

    const extractRows = (payload: any): Array<Record<string, any>> => {
      const roots = [payload, payload?.data, payload?.result, payload?.data?.data, payload?.result?.data].filter(
        Boolean,
      );

      for (const root of roots) {
        const candidates = [root?.tradeList, root?.list, root?.rows, root?.userTrades, root?.data, root];
        for (const candidate of candidates) {
          if (Array.isArray(candidate)) {
            return candidate;
          }
        }
      }

      return [];
    };

    const fetchRows = async (): Promise<Array<Record<string, any>>> => {
      const historyParams = omitUndefined({
        symbol: market.symbol,
        orderId,
        startTime: fromMilliseconds,
        endTime: toMilliseconds,
        limit: maxLimit,
      });

      const requestConfig: any = {
        params: historyParams,
        __suppressExchangeError: true,
        __suppressExchangeRateLimitError: true,
        headers: {
          'x-tealstreet-suppress-exchange-error': '1',
          'x-tealstreet-suppress-exchange-rate-limit-error': '1',
        },
      };
      const response = await this.xhr.get(ENDPOINTS.HISTORY_TRADES, requestConfig);
      return extractRows(response?.data);
    };

    try {
      const primaryRows = await fetchRows();

      const deduped = new Map<string, Execution>();
      for (const execution of parseRows(primaryRows)) {
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
      this.log(`fetchExecutions failed: ${err?.response?.data?.msg || err?.message}`, LogSeverity.Warning);
      return [];
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const limit = Math.min(opts.limit || 200, 200);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const {
      data: { data },
    } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.KLINE, {
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
      // 5 minutes timeout
      timeout: 5 * 60 * 1000,
    });

    if (!data || !Array.isArray(data)) {
      return [];
    }

    const candles: Candle[] = data.map(({ time, open, high, low, close, quoteVol }) => {
      return {
        timestamp: time / 1000,
        open: parseFloat(open),
        high: parseFloat(high),
        low: parseFloat(low),
        close: parseFloat(close),
        volume: parseFloat(quoteVol),
      };
    });

    return candles;
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    if (!this.symbolSettings.get(opts.symbol)) {
      try {
        this.updateSymbolMetadata(opts.symbol);
      } catch (_e) {}
    }
    return this.futuresWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.futuresWebsocket.listenOrderBook(symbol, callback);
  };

  fetchPositionMode = async () => {
    // Bitunix's GET /account/change_position_mode now returns
    // {code:1, msg:"Network Error"}, surfacing as a startup toast. The
    // /account endpoint already returns positionMode ("HEDGE" | "ONE_WAY"),
    // and fetchBalance reads it from the same response, so reuse it here
    // to avoid the broken endpoint entirely.
    const response = await this.xhr.get(ENDPOINTS.ACCOUNT, {
      params: { marginCoin: 'USDT' },
    });
    const account = this.extractAccountRow(response?.data);
    // Mirror fetchBalance's classification — treat any non-ONE_WAY value
    // (including missing/unexpected casing) as Hedged so the two helpers
    // stay in sync.
    return String(this.getFirstValue(account, ['positionMode', 'posMode']) || '').toUpperCase() !== 'ONE_WAY';
  };

  changePositionMode = async (hedged: boolean) => {
    if (this.store.positions.filter((p) => p.contracts > 0).length > 0) {
      this.emitAndLogError('Please close all positions before switching position mode');
      return;
    }

    try {
      await this.xhr.post(ENDPOINTS.HEDGE_MODE, {
        positionMode: hedged ? 'HEDGE' : 'ONE_WAY',
      });
      this.store.setSetting('isHedged', hedged);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }

    try {
      await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, {
        symbol,
        marginMode: isolated ? 'ISOLATION' : 'CROSS',
        marginCoin: 'USDT', // Required parameter according to docs
      });
    } catch (e: any) {
      this.emitAndLogError(e?.response?.data?.msg || e?.message);
      throw new Error(e?.response?.data?.msg || e?.message);
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number) => {
    const market = this.store.marketsBySymbol[symbol];
    const position = this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
          marginCoin: 'USDT',
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
      // Separate virtual TP/SL orders from regular orders
      const virtualTpSlOrders = orders.filter((o) => this.getAttachedVirtualTpslLeg(o));
      const regularOrders = orders.filter((o) => !this.getAttachedVirtualTpslLeg(o));

      // console.log('virtualTpSlOrders', virtualTpSlOrders);
      // console.log('regularOrders', regularOrders);

      // Handle virtual TP/SL orders - need to recreate parent orders
      const parentOrdersToRecreate = new Map<string, { parentOrder: Order; keepTp: boolean; keepSl: boolean }>();

      for (const virtualOrder of virtualTpSlOrders) {
        const parentOrderId = this.getVirtualTpslParentOrderId(virtualOrder.id);
        const isTP = this.getAttachedVirtualTpslLeg(virtualOrder) === 'tp';

        // Find parent order and related virtual orders
        const parentOrder = this.store.orders.find((o) => o.id === parentOrderId);
        if (!parentOrder) continue;

        const allRelatedOrders = this.store.orders.filter(
          (o) => o.id === parentOrderId || o.id.startsWith(`${parentOrderId}__`),
        );

        const tpOrder = allRelatedOrders.find((o) => o.id.includes('__tp'));
        const slOrder = allRelatedOrders.find((o) => o.id.includes('__sl'));

        // Determine what to keep when recreating
        const keepTp = tpOrder && !isTP && !virtualTpSlOrders.some((vo) => vo.id === tpOrder.id);
        const keepSl = slOrder && isTP && !virtualTpSlOrders.some((vo) => vo.id === slOrder.id);

        parentOrdersToRecreate.set(parentOrderId, {
          parentOrder,
          keepTp: !!keepTp,
          keepSl: !!keepSl,
        });
      }

      // Cancel parent orders that have virtual TP/SL being cancelled
      for (const [parentOrderId, { parentOrder }] of parentOrdersToRecreate) {
        await this.xhr.post(ENDPOINTS.CANCEL_ORDER, {
          symbol: parentOrder.symbol,
          orderList: [{ orderId: parentOrderId }],
        });
      }

      // Handle regular TP/SL orders (standalone, not virtual)
      const standaloneTPSLOrders = regularOrders.filter(
        (o) => o.type === OrderType.TakeProfit || o.type === OrderType.StopLoss,
      );

      for (const order of standaloneTPSLOrders) {
        await this.xhr.post(ENDPOINTS.CANCEL_ORDER_TP_SL, {
          symbol: order.symbol,
          orderId: this.getTpslRawOrderId(order),
        });
      }

      // Handle regular limit/market orders
      const limitMarketOrders = regularOrders.filter((o) => o.type === OrderType.Limit || o.type === OrderType.Market);

      const groupedBySymbol = groupBy(limitMarketOrders, 'symbol');
      for (const [symbol, symbolOrders] of Object.entries(groupedBySymbol)) {
        await this.xhr.post(ENDPOINTS.CANCEL_ORDER, {
          symbol,
          orderList: symbolOrders.map((o) => ({ orderId: o.id })),
        });
      }

      // Update store to mark all cancelled orders
      this.store.updateOrdersStatus(
        orders.map((o) => ({ id: o.id })),
        OrderStatus.Canceled,
      );

      // Recreate parent orders only if some TP/SL should be preserved.
      // Skip if: parent itself is being canceled, or both TP and SL are removed.
      for (const [parentOrderId, { parentOrder, keepTp, keepSl }] of parentOrdersToRecreate) {
        // Don't recreate if the parent order is also in the cancel set
        if (regularOrders.some((o) => o.id === parentOrderId)) continue;
        // Don't recreate if neither TP nor SL is being kept
        if (!keepTp && !keepSl) continue;

        const tpOrder = this.store.orders.find((o) => o.id === `${parentOrderId}__tp`);
        const slOrder = this.store.orders.find((o) => o.id === `${parentOrderId}__sl`);

        const newOrderOpts: Writable<PlaceOrderOpts> = {
          symbol: parentOrder.symbol,
          type: parentOrder.type,
          side: parentOrder.side,
          price: parentOrder.price,
          amount: parentOrder.amount,
          reduceOnly: parentOrder.reduceOnly || false,
        };

        if (keepTp && tpOrder) {
          newOrderOpts.takeProfit = tpOrder.price;
          newOrderOpts.tpTriggerType = tpOrder.tpTriggerType;
        }

        if (keepSl && slOrder) {
          newOrderOpts.stopLoss = slOrder.price;
          newOrderOpts.slTriggerType = slOrder.slTriggerType;
        }

        await this.placeOrder(newOrderOpts);
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    return await this.cancelOrders(this.store.orders.filter((o) => o.symbol === symbol));
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    const virtualTpslLeg = this.getAttachedVirtualTpslLeg(order);
    const isVirtualTp = virtualTpslLeg === 'tp';
    const isVirtualSl = virtualTpslLeg === 'sl';
    const isVirtualTpSl = !!virtualTpslLeg;
    const parentOrderId = isVirtualTpSl ? this.getVirtualTpslParentOrderId(order.id) : order.id;
    const targetOrder = isVirtualTpSl ? this.store.orders.find((candidate) => candidate.id === parentOrderId) : order;

    if (!targetOrder) {
      return rejectPreflight(`Bitunix cannot safely amend ${order.id}: reconciled parent order not found`);
    }

    if (isVirtualTpSl && 'amount' in update) {
      return rejectPreflight('Bitunix attached TP/SL quantity cannot be amended independently of its parent order');
    }

    if (isVirtualTpSl && !('price' in update)) {
      return rejectPreflight('Bitunix attached TP/SL amendment requires a trigger price');
    }

    if (targetOrder.type !== OrderType.Limit) {
      return rejectPreflight(
        `Bitunix native amendment supports only reconciled pending LIMIT orders; ${order.type} requires an ambiguous TP/SL or order-type route`,
      );
    }

    const market = this.store.marketsBySymbol[targetOrder.symbol];
    if (!market) {
      return rejectPreflight(`Market ${targetOrder.symbol} not found`);
    }

    const info = targetOrder.info;
    const reconciledOrderId = info?.orderId === undefined || info?.orderId === null ? '' : String(info.orderId);
    const reconciledOrderType = String(info?.orderType || info?.type || '').toUpperCase();
    if (reconciledOrderId !== targetOrder.id || reconciledOrderType !== 'LIMIT') {
      return rejectPreflight(
        `Bitunix cannot safely amend ${targetOrder.id}: exact pending LIMIT order identity is not reconciled`,
      );
    }

    if (typeof info?.reduceOnly !== 'boolean' || info.reduceOnly !== targetOrder.reduceOnly) {
      return rejectPreflight(`Bitunix cannot safely amend ${targetOrder.id}: reduce-only state is not reconciled`);
    }

    const adjustedQty = adjust('amount' in update ? update.amount : targetOrder.amount, market.precision.amount);
    const adjustedPrice = adjust(
      'price' in update && !isVirtualTpSl ? update.price : targetOrder.price,
      market.precision.price,
    );
    if (!Number.isFinite(adjustedQty) || adjustedQty <= 0) {
      return rejectPreflight('Bitunix amended order quantity must be greater than zero');
    }
    if (!Number.isFinite(adjustedPrice) || adjustedPrice <= 0) {
      return rejectPreflight('Bitunix amended LIMIT price must be greater than zero');
    }
    if (targetOrder.filled > 0 && adjustedQty <= targetOrder.filled) {
      return rejectPreflight('Bitunix amended order quantity must exceed the already-filled quantity');
    }

    const payload: Record<string, string> = {
      orderId: targetOrder.id,
      qty: `${adjustedQty}`,
      price: `${adjustedPrice}`,
    };

    const appendProtection = (prefix: 'tp' | 'sl', triggerPrice: unknown) => {
      if (triggerPrice === undefined || triggerPrice === null || triggerPrice === '') {
        return;
      }

      const adjustedTrigger = adjust(Number(triggerPrice), market.precision.price);
      if (!Number.isFinite(adjustedTrigger) || adjustedTrigger <= 0) {
        return rejectPreflight(`Bitunix ${prefix.toUpperCase()} trigger price is not reconciled`);
      }

      const stopTypeKey = `${prefix}StopType`;
      const orderTypeKey = `${prefix}OrderType`;
      const orderPriceKey = `${prefix}OrderPrice`;
      const stopType = String(info?.[stopTypeKey] || 'MARK_PRICE').toUpperCase();
      // The modify-order schema documents MARKET as the default when the
      // attached protection's execution type is omitted.
      const protectionOrderType = String(info?.[orderTypeKey] || 'MARKET').toUpperCase();

      if (stopType !== 'MARK_PRICE' && stopType !== 'LAST_PRICE') {
        return rejectPreflight(`Bitunix ${prefix.toUpperCase()} trigger type is not reconciled`);
      }
      if (protectionOrderType !== 'MARKET' && protectionOrderType !== 'LIMIT') {
        return rejectPreflight(`Bitunix ${prefix.toUpperCase()} execution type is not reconciled`);
      }

      payload[`${prefix}Price`] = `${adjustedTrigger}`;
      payload[stopTypeKey] = stopType;
      payload[orderTypeKey] = protectionOrderType;

      if (protectionOrderType === 'LIMIT') {
        const adjustedOrderPrice = adjust(Number(info?.[orderPriceKey]), market.precision.price);
        if (!Number.isFinite(adjustedOrderPrice) || adjustedOrderPrice <= 0) {
          return rejectPreflight(`Bitunix ${prefix.toUpperCase()} LIMIT execution price is not reconciled`);
        }
        payload[orderPriceKey] = `${adjustedOrderPrice}`;
      }
    };

    const requestedPrice = 'price' in update ? update.price : undefined;
    appendProtection('tp', isVirtualTp ? requestedPrice : info?.tpPrice);
    appendProtection('sl', isVirtualSl ? requestedPrice : info?.slPrice);

    try {
      const response = await this.unlimitedXHR.post(ENDPOINTS.MODIFY_ORDER, payload);
      const code = Number(response?.data?.code);
      if (code !== 0) {
        throw new Error(response?.data?.msg || `Bitunix modify order failed with code ${response?.data?.code}`);
      }

      const responseOrderId = response?.data?.data?.orderId;
      if (responseOrderId === undefined || responseOrderId === null || String(responseOrderId) !== targetOrder.id) {
        throw new Error(`Bitunix modify order response did not confirm order ${targetOrder.id}`);
      }

      // Bitunix documents the private WebSocket update as authoritative.
      // Keep the stable id and let the existing order-channel reconciliation
      // apply the accepted state instead of mutating the store optimistically.
      return [targetOrder.id];
    } catch (err: any) {
      const message = err?.response?.data?.msg || err?.message || 'Bitunix modify order failed';
      this.emitAndLogError(message);
      throw new Error(message);
    }
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    try {
      const payloads = this.formatCreateOrder(opts);
      return await this.placeOrderBatch(payloads);
    } catch (err: any) {
      const errMsg = err?.message || err?.response?.data?.msg || '';

      // Retry for position-related errors that can be fixed by refreshing:
      // - "Missing positionId" = local error, position found but ID missing
      // - "Position does not exist" = API error, stale positionId from cache
      // Don't retry for "nothing to reduce" = no position exists at all
      const isRetryablePositionError =
        errMsg.includes('Missing positionId') || errMsg.toLowerCase().includes('not exist');

      if (isRetryablePositionError) {
        console.warn(`[Bitunix] ${errMsg} - refreshing positions and retrying`);

        // Clear cached positionIds for this symbol — they're stale if we got "not exist"
        this.positionIdCache.delete(`${opts.symbol}:long`);
        this.positionIdCache.delete(`${opts.symbol}:short`);

        const freshPositions = await this.fetchPositions();
        this.store.update({ positions: freshPositions });

        // Retry — formatTPSLOrder will fall through to position.info.positionId
        const payloads = this.formatCreateOrder(opts);
        return await this.placeOrderBatch(payloads);
      }
      throw err;
    }
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    const requests = orders.flatMap((o) => this.formatCreateOrder(o));
    return await this.placeOrderBatch(requests);
  };

  updateSymbolMetadata = async (symbol: string) => {
    const {
      data: { data },
    } = await this.xhr.get<{ data: Record<string, any> }>(ENDPOINTS.LEVERAGE_MARGIN_MODE, {
      params: {
        symbol,
        marginCoin: 'USDT',
      },
    });

    const marginType = data?.marginMode === 'ISOLATION' ? MarginType.Isolated : MarginType.Cross;

    this.symbolSettings.set(symbol, {
      marginType,
      leverage: data?.leverage,
    });

    if (data) {
      this.store.update({
        positions: this.store.positions.map((position) => {
          if (position.symbol === symbol) {
            return {
              ...position,
              leverage: data.leverage,
              marginType,
            };
          }
          return position;
        }),
      });
    }
  };

  /**
   * Reliably get the Bitunix positionId for a given symbol and side.
   * Falls back to the positionIdCache if position.info is missing.
   */
  private getPositionId = (symbol: string, positionSide: PositionSide): string | undefined => {
    const position = this.store.positions.find(
      (p) => p.symbol === symbol && p.side === positionSide && p.contracts > 0,
    );

    // Try info.positionId first, then fall back to cache
    return position?.info?.positionId || this.positionIdCache.get(`${symbol}:${positionSide}`);
  };

  private formatCreateOrder = (opts: PlaceOrderOpts) => {
    if (opts.type === OrderType.TrailingStopLoss) {
      throw new Error('Trailing stop loss is not supported');
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

    // For TP/SL orders, we need to use a different endpoint
    if (isStopOrTP) {
      return this.formatTPSLOrder(opts);
    }

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);

    // We use price only for limit orders
    // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    const reduceOnly = opts.reduceOnly || false;
    const effect = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    // Get positions to determine the tradeSide and positionId
    let tradeSide = 'OPEN';
    let positionId;

    if (reduceOnly) {
      tradeSide = 'CLOSE';

      const positionSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
      const position = this.store.positions.find(
        (p) => p.symbol === opts.symbol && p.side === positionSide && p.contracts > 0,
      );

      if (!position) {
        // No position exists at all - don't retry, this is a genuine "no position" situation.
        // emitAndLogError before throwing so the toast pipeline fires
        // (controller → handleErrWarning → toast). A raw `throw` bypasses
        // the emitter and the user sees nothing — see exchanges/CLAUDE.md
        // "Pre-flight validation throws must emit first".
        const noPositionMsg = `No ${positionSide} position open for ${opts.symbol} - nothing to reduce`;
        this.emitAndLogError(noPositionMsg);
        throw new Error(noPositionMsg);
      }

      // Position exists, try to get positionId from info or cache
      positionId = position.info?.positionId || this.positionIdCache.get(`${opts.symbol}:${positionSide}`);

      if (!positionId) {
        // Position exists but positionId is missing - this CAN be fixed by refreshing.
        // Same emit-before-throw rule.
        const missingIdMsg = `Missing positionId for ${opts.symbol} (side: ${positionSide}) - cannot place reduce-only order without a positionId`;
        this.emitAndLogError(missingIdMsg);
        throw new Error(missingIdMsg);
      }
    }

    const req = omitUndefined({
      symbol: opts.symbol,
      qty: amount ? `${amount}` : undefined,
      price: price ? `${price}` : undefined,
      side: inverseObj(ORDER_SIDE)[opts.side],
      tradeSide,
      positionId: positionId,
      orderType: inverseObj(ORDER_TYPE)[opts.type],
      effect,
      clientId: 'ts_' + uuid(),
      reduceOnly,

      // Take profit parameters
      ...(opts.takeProfit && {
        tpPrice: `${opts.takeProfit}`,
        tpStopType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE',
        tpOrderType: 'MARKET',
      }),

      // Stop loss parameters
      ...(opts.stopLoss && {
        slPrice: `${opts.stopLoss}`,
        slStopType: opts.slTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE',
        slOrderType: 'MARKET',
      }),
    });

    // Cache TIF by clientId for REST API workaround
    if (req.clientId && opts.timeInForce) {
      this.orderTifCache.set(req.clientId, opts.timeInForce);
    }

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;
    const lotSize = adjust((amount - rest) / lots, pAmount);

    const payloads: Array<Record<string, any>> = times(lots, () => ({
      ...req,
      qty: `${lotSize}`,
    }));

    if (rest) {
      payloads.push({ ...req, qty: `${rest}` });
    }

    // For Bitunix, we'll handle TP/SL in the main order
    // instead of creating separate orders like in the old implementation

    return payloads;
  };

  private formatTPSLOrder = (opts: PlaceOrderOpts) => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    // Find the position to apply TP/SL to
    const positionSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const position = this.store.positions.find(
      (p) => p.symbol === opts.symbol && p.side === positionSide && p.contracts > 0,
    );

    if (!position) {
      return rejectPreflight(`No open ${positionSide} position found for ${opts.symbol}`);
    }

    // Prefer WS-cached positionId (updated immediately on position events)
    // over REST-polled position.info which can be stale after close+reopen
    const resolvedPositionId = this.positionIdCache.get(`${opts.symbol}:${positionSide}`) || position.info?.positionId;

    if (!resolvedPositionId) {
      return rejectPreflight(`Missing positionId for ${opts.symbol} TP/SL order`);
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      return rejectPreflight(`Market ${opts.symbol} not found`);
    }

    const pPrice = market.precision.price;

    // Check if this is a full position TP/SL order (amount is 0 or not specified)
    const isFullPositionOrder = !opts.amount || opts.amount === 0;

    // If amount is 0 or not specified, use the position TP/SL endpoint
    // This will close the entire position when triggered
    if (isFullPositionOrder) {
      if (!opts.price) {
        return rejectPreflight('Price is required for Position TP/SL orders');
      }

      const price = adjust(opts.price, pPrice);

      // Find existing TP/SL orders for this position to maintain both TP and SL when updating one
      const existingTpslOrders = this.store.orders.filter(
        (o) =>
          (o.type === OrderType.TakeProfit || o.type === OrderType.StopLoss) &&
          o.symbol === opts.symbol &&
          o.side === opts.side &&
          o.info?.positionId === resolvedPositionId,
      );

      // Extract current TP and SL values
      let currentTpPrice: string | undefined;
      let currentTpStopType: string | undefined;
      let currentSlPrice: string | undefined;
      let currentSlStopType: string | undefined;

      existingTpslOrders.forEach((order) => {
        if (order.type === OrderType.TakeProfit) {
          currentTpPrice = order.price.toString();
          currentTpStopType = order.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE';
        } else if (order.type === OrderType.StopLoss) {
          currentSlPrice = order.price.toString();
          currentSlStopType = order.slTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE';
        }
      });

      // Format according to the Position TP/SL API
      const payload: Record<string, any> = {
        symbol: opts.symbol,
        positionId: resolvedPositionId,
      };

      // For TakeProfit orders
      if (opts.type === OrderType.TakeProfit) {
        // Update TP values
        payload.tpPrice = `${price}`;
        payload.tpStopType = opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE';

        // Keep existing SL values if any
        if (currentSlPrice) {
          payload.slPrice = currentSlPrice;
          payload.slStopType = currentSlStopType;
        }
      }
      // For StopLoss orders
      else if (opts.type === OrderType.StopLoss) {
        // Update SL values
        payload.slPrice = `${price}`;
        payload.slStopType = opts.slTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE';

        // Keep existing TP values if any
        if (currentTpPrice) {
          payload.tpPrice = currentTpPrice;
          payload.tpStopType = currentTpStopType;
        }
      }

      return [
        {
          ...payload,
          isPositionOrder: true, // Flag to identify position orders in placeOrderBatch
        },
      ];
    }

    // For partial position TP/SL orders, use the regular TP/SL endpoint
    if (!opts.price) {
      return rejectPreflight('Price is required for TP/SL orders');
    }

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);
    const price = adjust(opts.price, pPrice);

    // Format according to the regular TP/SL API
    const payload = {
      symbol: opts.symbol,
      positionId: resolvedPositionId,
    };

    // For TakeProfit orders
    if (opts.type === OrderType.TakeProfit) {
      Object.assign(payload, {
        tpPrice: `${price}`,
        tpStopType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE',
        tpOrderType: 'MARKET',
        tpQty: `${amount}`,
      });
    }
    // For StopLoss orders
    else if (opts.type === OrderType.StopLoss) {
      Object.assign(payload, {
        slPrice: `${price}`,
        slStopType: opts.slTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'LAST_PRICE',
        slOrderType: 'MARKET',
        slQty: `${amount}`,
      });
    }

    return [payload];
  };

  private placeOrderBatch = async (payloads: any[]) => {
    const orderIds = [] as string[];

    // Separate position TP/SL orders from other orders
    const positionTpslOrders = payloads.filter((order) => order.isPositionOrder);

    // Separate regular TP/SL orders from regular orders
    const remainingPayloads = payloads.filter((order) => !order.isPositionOrder);
    const tpslOrders = remainingPayloads.filter(
      (order) =>
        order.orderType !== 'LIMIT' &&
        (order.hasOwnProperty('tpPrice') ||
          order.hasOwnProperty('slPrice') ||
          order.hasOwnProperty('tpQty') ||
          order.hasOwnProperty('slQty')),
    );

    const regularOrders = remainingPayloads.filter((order) => !tpslOrders.includes(order));

    // Bitunix API responses are { code: 0, data: { orderId: "..." }, msg: "Success" }
    // Axios destructures response.data = the full body, so orderId is at data.data.orderId

    // Process position TP/SL orders first
    for (const order of positionTpslOrders) {
      try {
        // Clean up the payload by removing the flag
        const payload = { ...order };
        delete payload.isPositionOrder;

        const { data } = await this.unlimitedXHR.post(ENDPOINTS.POSITION_TP_SL_ORDER, payload);

        // Bitunix returns HTTP 200 even for errors; check the code field
        if (data?.code !== 0) {
          const msg = data?.msg || 'Unknown error';
          this.emitAndLogError(msg);
          throw new Error(msg);
        }

        const resultOrderId = data?.data?.orderId;
        if (resultOrderId) {
          orderIds.push(resultOrderId);
        } else {
          orderIds.push(order.clientId || uuid());
        }
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    // Process regular TP/SL orders individually (they can't be batched)
    for (const order of tpslOrders) {
      try {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.TP_SL_ORDER, order);

        // Bitunix returns HTTP 200 even for errors; check the code field
        if (data?.code !== 0) {
          const msg = data?.msg || 'Unknown error';
          this.emitAndLogError(msg);
          throw new Error(msg);
        }

        const resultOrderId = data?.data?.orderId;
        if (resultOrderId) {
          orderIds.push(resultOrderId);
        } else {
          orderIds.push(order.clientId || uuid());
        }
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    // Process regular orders with batching
    const ordersBySymbol = groupBy(regularOrders, 'symbol');
    for (const [symbol, symbolOrders] of Object.entries(ordersBySymbol)) {
      // Process orders in chunks of 5 per symbol — Bitunix's batch_order
      // endpoint caps at 5 legs and rejects the whole batch with
      // "batch order max size:5" if exceeded.
      const lots = chunk(symbolOrders, 5);

      // Throttle between batch chunks — Bitunix returns `"request too
      // frequently"` if multiple batch_order POSTs land back-to-back on
      // unlimitedXHR (which intentionally has no client-side rate limit so
      // single order placement stays low-latency). Empirically: 150 ms and
      // 500 ms both still hit the limiter on a 19-leg scaled order
      // (4 batches). 1 s keeps the effective rate at ~5 orders/sec.
      // **Do not auto-retry on `"request too frequently"`** — that would
      // risk double-placing legs on a delayed-but-accepted response.
      // Apply only between chunks, never before the first.
      let chunkIndex = 0;

      for (const lot of lots) {
        if (chunkIndex > 0) await sleep(1000);
        chunkIndex++;
        if (lot.length === 1) {
          try {
            const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, lot[0]);
            if (data?.code && data.code !== 0) {
              throw new Error(data.msg || `Order failed with code ${data.code}`);
            }
            orderIds.push(data?.data?.orderId || lot[0].clientId || uuid());
          } catch (err: any) {
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
            throw err;
          }
        }

        if (lot.length > 1) {
          try {
            const { data } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, {
              symbol,
              orderList: lot.map((order) => omit(order, 'symbol')),
            });

            // Bitunix returns HTTP 200 even for whole-batch rejections
            // (e.g. rate-limit / server reject), with `{code:<non-zero>, msg}`
            // and no successList/failureList. Mirror the single-order branch:
            // emit the error and abort this chunk so the caller doesn't get
            // back empty IDs that look like "no order IDs returned".
            if (data?.code && data.code !== 0) {
              throw new Error(data.msg || `Batch order failed with code ${data.code}`);
            }

            // Bitunix batch response shape: `data.data = { successList: [...], failureList: [...] }`
            // — NOT a flat array. Each successList entry is `{ orderId, clientId }`.
            // failureList carries `{ errorMsg, errorCode, ... }` per rejected leg.
            const batchResults = data?.data;
            const successList: any[] = Array.isArray(batchResults?.successList)
              ? batchResults.successList
              : Array.isArray(batchResults)
                ? batchResults.filter((o: any) => !o.code)
                : [];
            const failureList: any[] = Array.isArray(batchResults?.failureList)
              ? batchResults.failureList
              : Array.isArray(batchResults)
                ? batchResults.filter((o: any) => o.code)
                : [];
            for (const o of successList) {
              orderIds.push(o.orderId || o.clientId || o.clientOrderId || uuid());
            }
            for (const o of failureList) {
              this.emitAndLogError(o.errorMsg || o.msg || `Order rejected (code ${o.errorCode ?? o.code})`);
            }
          } catch (err: any) {
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
          }
        }
      }
    }

    return orderIds;
  };
}
