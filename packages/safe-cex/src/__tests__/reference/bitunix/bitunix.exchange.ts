import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import groupBy from 'lodash/groupBy';
import omit from 'lodash/omit';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';

import { parsePrecision } from '@tealstreet/ccxt-compat/src/functions/ccxt_exchange';
import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Candle,
  ClosedPnlEntry,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalOrder,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderHistoryOptions,
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
import { createAPI } from './bitunix.api';
import { mapBitunixPositionHistory } from '../../../journal/bitunix.position-adapter';
import { mapBitunixClosedPosition, mapBitunixTradeToIncomeRecords } from '../../../exchanges/bitunix/bitunix.income';
import { mapBitunixHistoricalOrder } from './bitunix.order-history';
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
import { BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION, BitunixPublicWebsocket } from './bitunix.ws-public';

// TODO: translate our err messages
export class BitunixExchange extends BaseExchange {
  // Bitunix publishes no clientId length limit; reject only empty or whitespace-bearing IDs.
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Bitunix', /^\S+$/);

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

  // The private order channel does not publish reduceOnly. Retain the value
  // submitted with a Tealstreet clientId so freshly-created orders are
  // amendable before the first REST reconciliation.
  orderReduceOnlyCache: Map<string, boolean> = new Map();

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

  private isApiTradingSupported = (value: unknown) => {
    if (value === undefined || value === null || value === '') {
      return true;
    }
    if (typeof value === 'boolean') {
      return value;
    }
    if (typeof value === 'number') {
      return value !== 0;
    }
    if (typeof value === 'string') {
      const normalized = value.trim().toLowerCase();
      return !['false', '0', 'no', 'n', 'disabled', 'unsupported'].includes(normalized);
    }
    return true;
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

  private isStandaloneTpslOrder = (order: Order) => {
    return (
      (order.type === OrderType.TakeProfit || order.type === OrderType.StopLoss) &&
      !this.getAttachedVirtualTpslLeg(order)
    );
  };

  private isPositionTpslOrder = (order: Order) => {
    return String(order.info?.type || '').toUpperCase() === 'POSITION_TPSL' || order.amount === 0;
  };

  private getBitunixTriggerType = (triggerType: OrderStopTriggerType | undefined, rawTriggerType: unknown) => {
    const raw = String(rawTriggerType || '').toUpperCase();
    if (raw === 'MARK' || raw === 'MARK_PRICE') {
      return 'MARK_PRICE';
    }
    if (raw === 'LAST' || raw === 'LAST_PRICE') {
      return 'LAST_PRICE';
    }
    return triggerType === OrderStopTriggerType.LastPrice ? 'LAST_PRICE' : 'MARK_PRICE';
  };

  private getInternalTriggerType = (rawTriggerType: unknown) => {
    const raw = String(rawTriggerType || '').toUpperCase();
    return raw === 'LAST' || raw === 'LAST_PRICE' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice;
  };

  private isUnsupportedOrderOperation = (message: string) => {
    return message.toLowerCase().includes('order does not support this operation');
  };

  getBitunixReduceOnly = (rawReduceOnly: unknown, clientId: unknown, fallback = false) => {
    if (typeof rawReduceOnly === 'boolean') {
      return rawReduceOnly;
    }
    if (typeof rawReduceOnly === 'string') {
      if (rawReduceOnly.toLowerCase() === 'true') return true;
      if (rawReduceOnly.toLowerCase() === 'false') return false;
    }

    const cached =
      clientId === undefined || clientId === null ? undefined : this.orderReduceOnlyCache.get(String(clientId));
    return cached ?? fallback;
  };

  private getTpslLegSnapshot = (order: Order, leg: 'tp' | 'sl') => {
    const rawOrderId = this.getTpslRawOrderId(order);
    const positionId = order.info?.positionId;
    const sameRawOrderId = (candidate: Order) => this.getTpslRawOrderId(candidate) === rawOrderId;
    const samePositionId = (candidate: Order) =>
      positionId !== undefined && positionId !== null && candidate.info?.positionId === positionId;
    const legType = leg === 'tp' ? OrderType.TakeProfit : OrderType.StopLoss;
    const currentOrderMatchesLeg = order.type === legType;

    const relatedOrder =
      currentOrderMatchesLeg && this.isStandaloneTpslOrder(order)
        ? order
        : this.store.orders.find(
            (candidate) =>
              candidate.symbol === order.symbol &&
              candidate.type === legType &&
              this.isStandaloneTpslOrder(candidate) &&
              (sameRawOrderId(candidate) || samePositionId(candidate)),
          );

    const info = relatedOrder?.info || order.info || {};
    const priceField = leg === 'tp' ? 'tpPrice' : 'slPrice';
    const qtyField = leg === 'tp' ? 'tpQty' : 'slQty';
    const stopTypeField = leg === 'tp' ? 'tpStopType' : 'slStopType';
    const orderTypeField = leg === 'tp' ? 'tpOrderType' : 'slOrderType';
    const orderPriceField = leg === 'tp' ? 'tpOrderPrice' : 'slOrderPrice';
    const triggerType = leg === 'tp' ? relatedOrder?.tpTriggerType : relatedOrder?.slTriggerType;
    const price = relatedOrder?.price ?? this.parseFiniteNumber(info[priceField], NaN);
    const qty = relatedOrder?.amount ?? this.parseFiniteNumber(info[qtyField], NaN);

    if (!Number.isFinite(price) || price <= 0) {
      return undefined;
    }

    return {
      price,
      qty,
      stopType: this.getBitunixTriggerType(triggerType, info[stopTypeField]),
      orderType: String(info[orderTypeField] || 'MARKET').toUpperCase(),
      orderPrice: this.parseFiniteNumber(info[orderPriceField], NaN),
    };
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
            this.isApiTradingSupported(v(m, 'isApiSupported')),
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
                // The catalogue sends a decimal string; consumers compare numerically.
                max: Number(v(m, 'maxLimitOrderVolume')),
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

      // Leverage and margin mode are per symbol: the flat placeholder side
      // inherits them from the real leg before cached settings or defaults.
      const createFakePosition = (m: Market, side: PositionSide, positionMode: PositionMode) => {
        const real = formattedPositions.find((p) => p.symbol === m.symbol);
        const symbolSettings = real ?? this.symbolSettings.get(m.symbol);
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
        const rawOrderType = String(this.getFirstValue(o, ['orderType', 'type']) || '').toUpperCase();
        const reduceOnly = this.getBitunixReduceOnly(o.reduceOnly, o.clientId);
        const orderInfo = { ...o, reduceOnly };

        const baseOrder = {
          id: v(o, 'orderId'),
          clientOrderId: o.clientId || undefined,
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: ORDER_TYPE[rawOrderType],
          side: ORDER_SIDE[String(o.side || '').toUpperCase()],
          price: parseFloat(o.price), // || parseFloat(v(o, 'stopPrice')),
          amount: parseFloat(v(o, 'qty')),
          reduceOnly,
          filled: parseFloat(v(o, 'tradeQty')),
          lastUpdate: v(o, 'mtime'),
          remaining: subtract(v(o, 'qty'), v(o, 'tradeQty')),
          takeProfitPrice: parseFloat(v(o, 'tpOrderPrice')),
          stopLossPrice: parseFloat(v(o, 'slOrderPrice')),
          timeInForce,
          info: orderInfo,
          // Bitunix omits stop type when it's MarkPrice (the default)
          // Attached protection carries a trigger type only when that leg exists.
          tpTriggerType: this.hasFiniteTriggerPrice(o.tpPrice)
            ? ORDER_TRIGGER_TYPE[v(o, 'tpStopType')] || OrderStopTriggerType.MarkPrice
            : undefined,
          slTriggerType: this.hasFiniteTriggerPrice(o.slPrice)
            ? ORDER_TRIGGER_TYPE[v(o, 'slStopType')] || OrderStopTriggerType.MarkPrice
            : undefined,
        };

        const subList = [baseOrder];

        // Create virtual TP/SL orders if they exist on the limit order
        if (this.hasFiniteTriggerPrice(o.tpPrice)) {
          subList.push({
            id: `${v(o, 'orderId')}__tp`,
            // Virtual legs never carry the parent's caller ID.
            clientOrderId: undefined,
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
            info: orderInfo,
            tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'tpStopType')] || OrderStopTriggerType.MarkPrice,
            slTriggerType: OrderStopTriggerType.MarkPrice,
          });
        }

        if (this.hasFiniteTriggerPrice(o.slPrice)) {
          subList.push({
            id: `${v(o, 'orderId')}__sl`,
            // Virtual legs never carry the parent's caller ID.
            clientOrderId: undefined,
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
            info: orderInfo,
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

  /**
   * Ledger from `get_history_trades`, closed positions from `get_history_positions`.
   * See `bitunix.income.ts` for the verified sign conventions and the funding gap.
   *
   * Both windows are enforced CLIENT-SIDE as well as sent, because a sibling venue
   * (blofin) silently ignores its server-side time filter on the fills endpoint while
   * honouring it on positions — a 30-day request there returned six months. Enforcing
   * locally costs nothing and removes the need to trust each endpoint individually.
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const rows = await this.fetchBitunixHistoryPages(
      ENDPOINTS.HISTORY_TRADES,
      opts?.startTime,
      opts?.endTime,
      (r) => parseInt(String(r.ctime ?? '0'), 10),
      (r) => String(r.tradeId ?? ''),
      opts?.symbol,
    );

    const records = rows.flatMap((row) => mapBitunixTradeToIncomeRecords(row));
    return records.sort((a, b) => b.timestamp - a.timestamp);
  };

  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    return await this.fetchIncomeHistory({ startTime, endTime, incomeType });
  };

  /**
   * RAW history-position rows. Public because the journal adapter reads fields
   * `ClosedPnlEntry` does not carry, and the audit harness was scraping them off the URL.
   * `fetchBitunixHistoryPages` already owned the paging, so this is the same walk both
   * consumers use rather than a second one.
   */
  fetchClosedPositionRows = async (opts: HistoricalTradeOptions): Promise<Array<Record<string, any>>> => {
    if (this.isPublicDataOnly) return [];
    return await this.fetchBitunixHistoryPages(
      ENDPOINTS.HISTORY_POSITIONS,
      opts.from,
      opts.to,
      (r) => parseInt(String(r.mtime ?? r.ctime ?? '0'), 10),
      (r) => String(r.positionId ?? ''),
      opts.symbol,
    );
  };

  /** Journal records from the REPORTED closed axis — bitunix folds to nothing from fills. */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) => {
    const rows = await this.fetchClosedPositionRows({ from: opts.from, to: opts.to } as HistoricalTradeOptions);
    const records = [];
    for (const row of rows) {
      const record = mapBitunixPositionHistory(row, {
        account: opts.account,
        resolveSettleCurrency: (symbol: string) => this.store?.marketsBySymbol?.[symbol]?.quote || 'USDT',
      });
      if (record) records.push(record);
    }
    return records;
  };

  fetchClosedPnl = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const rows = await this.fetchBitunixHistoryPages(
      ENDPOINTS.HISTORY_POSITIONS,
      opts.from,
      opts.to,
      (r) => parseInt(String(r.mtime ?? r.ctime ?? '0'), 10),
      (r) => String(r.positionId ?? ''),
      opts.symbol,
    );

    return rows
      .map((row) => mapBitunixClosedPosition(row, opts.accountName || ''))
      .filter((entry): entry is ClosedPnlEntry => entry !== null)
      .sort((a, b) => b.closedTime - a.closedTime);
  };

  /**
   * Historical orders, including ones that never filled — see `bitunix.order-history.ts`.
   *
   * **This endpoint does NOT share the `skip` pagination of `get_history_trades`.** On
   * `get_history_orders`, `skip=100` returns a byte-identical first page and `status` is
   * ignored outright (`status=CANCELED` came back 100/100 `FILLED`). Only `symbol` and the
   * `startTime`/`endTime` window are honoured. So this walks the window backwards instead
   * of reusing `fetchBitunixHistoryPages`; routing it through the shared pager silently
   * caps the axis at 100 orders and reads as a small account.
   *
   * The cursor is the oldest `ctime` seen, NOT `oldest - 1`. Trimming a millisecond off
   * the boundary would drop any same-millisecond order that did not fit on the page —
   * the same defect that cost real fills on hyperliquid. Overlap is absorbed by the id
   * dedupe, and a page that contributes nothing new ends the walk.
   */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const MAX_PAGES = 50;
    const PAGE_LIMIT = 100;

    try {
      const orders: HistoricalOrder[] = [];
      const seen = new Set<string>();
      let endTime = opts?.to;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        const { data } = await this.xhr.get(ENDPOINTS.HISTORY_ORDERS, {
          params: omitUndefined({
            symbol: opts?.symbol,
            startTime: opts?.from,
            endTime,
            limit: PAGE_LIMIT,
          }),
        });

        const rows: Array<Record<string, any>> = Array.isArray(data?.data?.orderList) ? data.data.orderList : [];
        if (rows.length === 0) break;

        let oldest = Number.POSITIVE_INFINITY;
        // Progress is measured in RAW ids, not in mapped orders. A full page the mapper
        // rejects — an unrecognised side, a missing ctime — would otherwise read as "no
        // more history" and truncate the walk while older orders still exist, which is
        // indistinguishable from an account that simply stops there.
        let freshRows = 0;

        for (const row of rows) {
          const createdTime = parseInt(String(row.ctime ?? '0'), 10);
          if (createdTime) oldest = Math.min(oldest, createdTime);

          const rawId = String(row.orderId ?? '');
          if (!rawId || seen.has(rawId)) continue;
          seen.add(rawId);
          freshRows += 1;

          const order = mapBitunixHistoricalOrder(row);
          if (order) orders.push(order);
        }

        if (rows.length < PAGE_LIMIT || freshRows === 0 || !Number.isFinite(oldest)) break;
        if (opts?.from && oldest <= opts.from) break;
        endTime = oldest;
      }

      if (page >= MAX_PAGES) {
        this.log(
          `fetchHistoricalOrders stopped at the ${MAX_PAGES}-page cap; older orders were not fetched`,
          LogSeverity.Warning,
        );
      }

      const sorted = orders.sort((a, b) => b.createdTime - a.createdTime);
      return opts?.limit && opts.limit > 0 ? sorted.slice(0, opts.limit) : sorted;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  private fetchBitunixHistoryPages = async (
    endpoint: string,
    startTime: number | undefined,
    endTime: number | undefined,
    timeOf: (row: Record<string, any>) => number,
    idOf: (row: Record<string, any>) => string,
    symbol?: string,
  ): Promise<Array<Record<string, any>>> => {
    const MAX_PAGES = 100;
    const PAGE_LIMIT = 100;
    const collected: Array<Record<string, any>> = [];
    const seen = new Set<string>();

    try {
      // `skip` is the pagination param. `page` and `offset` are SILENTLY IGNORED — both
      // return the identical newest page, so a loop built on either stops after one page
      // (every row deduped) and the ledger looks like a 100-fill account. Verified live.
      for (let page = 0; page < MAX_PAGES; page += 1) {
        const { data } = await this.xhr.get(endpoint, {
          params: omitUndefined({
            symbol,
            startTime,
            endTime,
            limit: PAGE_LIMIT,
            skip: page === 0 ? undefined : page * PAGE_LIMIT,
          }),
        });

        const body = data?.data;
        // The row key is per-endpoint. Getting it wrong does NOT error: bitunix answers a
        // nonexistent path with `code: 404`, but a real path read under the wrong key
        // yields `code: 0` and nothing, which reads as an empty account.
        const rows: Array<Record<string, any>> = Array.isArray(body)
          ? body
          : Array.isArray(body?.positionList)
            ? body.positionList
            : Array.isArray(body?.tradeList)
              ? body.tradeList
              : Array.isArray(body?.orderList)
                ? body.orderList
                : [];

        if (rows.length === 0) break;

        let added = 0;
        let reachedStart = false;
        for (const row of rows) {
          const ts = timeOf(row);
          if (startTime && ts && ts < startTime) {
            reachedStart = true;
            continue;
          }
          if (endTime && ts && ts > endTime) continue;

          const id = idOf(row);
          if (id && seen.has(id)) continue;
          if (id) seen.add(id);
          collected.push(row);
          added += 1;
        }

        // Newest-first, so the first page reaching past the window start ends the walk.
        if (reachedStart || rows.length < PAGE_LIMIT || added === 0) break;

        if (page === MAX_PAGES - 1) {
          this.log(
            `bitunix history paging hit the ${MAX_PAGES}-page cap on ${endpoint}; older rows were not fetched`,
            LogSeverity.Warning,
          );
        }
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return collected;
    }

    return collected;
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
            // /futures/trade/get_history_trades returns a tradeId per fill; orderId
            // repeats across the partial fills of one order and cannot dedupe them.
            id: row.tradeId != null && row.tradeId !== '' ? String(row.tradeId) : undefined,
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
        // tradeId identifies an execution; equal partial fills of one order are distinct.
        const key = execution.id ?? `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
        deduped.set(key, execution);
      }

      return [...deduped.values()].sort((a, b) => b.timestamp - a.timestamp).slice(0, maxLimit);
    } catch (err: any) {
      this.log(`fetchHistoricalFills failed: ${err?.response?.data?.msg || err?.message}`, LogSeverity.Warning);
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
      const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, {
        symbol,
        marginMode: isolated ? 'ISOLATION' : 'CROSS',
        marginCoin: 'USDT', // Required parameter according to docs
      });
      // HTTP 200 may still carry a business rejection; only an accepted change
      // is applied, to both legs like setLeverage.
      if (data?.code !== undefined && Number(data.code) !== 0) throw new Error(data?.msg || 'Bitunix rejected margin mode');
      const marginType = isolated ? MarginType.Isolated : MarginType.Cross;
      const settings = this.symbolSettings.get(symbol);
      if (settings) this.symbolSettings.set(symbol, { ...settings, marginType });
      this.store.updatePositions([
        [{ id: createPositionId(symbol, PositionSide.Long) }, { marginType }],
        [{ id: createPositionId(symbol, PositionSide.Short) }, { marginType }],
      ]);
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

  private recreateAttachedTpslParentOrder = async (
    targetOrder: Order,
    virtualTpslLeg: 'tp' | 'sl',
    payload: Record<string, string>,
  ) => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    const replacementPrice = this.parseFiniteNumber(payload.price, NaN);
    const replacementAmount = this.parseFiniteNumber(payload.qty, NaN);
    if (!Number.isFinite(replacementPrice) || replacementPrice <= 0) {
      return rejectPreflight('Bitunix cancel/recreate fallback requires a valid parent LIMIT price');
    }
    if (!Number.isFinite(replacementAmount) || replacementAmount <= 0) {
      return rejectPreflight('Bitunix cancel/recreate fallback requires a valid parent quantity');
    }

    const replacementOrder: Writable<PlaceOrderOpts> = {
      symbol: targetOrder.symbol,
      type: targetOrder.type,
      side: targetOrder.side,
      price: replacementPrice,
      amount: replacementAmount,
      reduceOnly: targetOrder.reduceOnly || false,
      timeInForce: targetOrder.timeInForce,
    };

    const appendProtection = (prefix: 'tp' | 'sl') => {
      const triggerPrice = this.parseFiniteNumber(payload[`${prefix}Price`], NaN);
      if (!Number.isFinite(triggerPrice) || triggerPrice <= 0) {
        return;
      }

      const orderType = String(payload[`${prefix}OrderType`] || 'MARKET').toUpperCase();
      if (orderType !== 'MARKET') {
        return rejectPreflight(
          `Bitunix cannot cancel/recreate attached ${prefix.toUpperCase()} with LIMIT execution semantics`,
        );
      }

      if (prefix === 'tp') {
        replacementOrder.takeProfit = triggerPrice;
        replacementOrder.tpTriggerType = this.getInternalTriggerType(payload.tpStopType);
      } else {
        replacementOrder.stopLoss = triggerPrice;
        replacementOrder.slTriggerType = this.getInternalTriggerType(payload.slStopType);
      }
    };

    appendProtection('tp');
    appendProtection('sl');

    if (replacementOrder.takeProfit === undefined && replacementOrder.stopLoss === undefined) {
      return rejectPreflight('Bitunix cancel/recreate fallback requires at least one attached TP/SL leg');
    }

    const cancelResponse = await this.xhr.post(ENDPOINTS.CANCEL_ORDER, {
      symbol: targetOrder.symbol,
      orderList: [{ orderId: targetOrder.id }],
    });
    const cancelCode = Number(cancelResponse?.data?.code);
    if (cancelCode !== 0) {
      throw new Error(cancelResponse?.data?.msg || `Bitunix cancel order failed with code ${cancelResponse?.data?.code}`);
    }

    this.store.updateOrdersStatus(
      this.store.orders
        .filter((candidate) => candidate.id === targetOrder.id || candidate.id.startsWith(`${targetOrder.id}__`))
        .map((candidate) => ({ id: candidate.id })),
      OrderStatus.Canceled,
    );

    const replacementIds = await this.placeOrder(replacementOrder);
    const replacementParentId = replacementIds[0];
    return replacementParentId ? [`${replacementParentId}__${virtualTpslLeg}`] : [];
  };

  private updateStandaloneTpslOrder = async (order: Order, update: UpdateOrderOpts['update']) => {
    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) {
      return rejectPreflight(`Market ${order.symbol} not found`);
    }

    const leg = order.type === OrderType.TakeProfit ? 'tp' : 'sl';
    const isPositionOrder = this.isPositionTpslOrder(order);
    const rawOrderId = this.getTpslRawOrderId(order);
    const positionId = order.info?.positionId;

    if (isPositionOrder && 'amount' in update) {
      return rejectPreflight('Bitunix full-position TP/SL quantity cannot be amended');
    }

    const payload: Record<string, string> = isPositionOrder
      ? {
          symbol: order.symbol,
          positionId: String(positionId || ''),
        }
      : {
          orderId: rawOrderId,
        };

    if (isPositionOrder && !positionId) {
      return rejectPreflight(`Bitunix cannot amend ${order.id}: missing positionId`);
    }

    const appendLeg = (targetLeg: 'tp' | 'sl', requestedOrder?: Order) => {
      const snapshot = this.getTpslLegSnapshot(requestedOrder || order, targetLeg);
      if (!snapshot) {
        return;
      }

      const prefix = targetLeg;
      const triggerPrice =
        targetLeg === leg && 'price' in update ? adjust(update.price, market.precision.price) : snapshot.price;
      if (!Number.isFinite(triggerPrice) || triggerPrice <= 0) {
        return rejectPreflight(`Bitunix ${prefix.toUpperCase()} trigger price is invalid`);
      }

      payload[`${prefix}Price`] = `${triggerPrice}`;
      payload[`${prefix}StopType`] = snapshot.stopType;

      if (isPositionOrder) {
        return;
      }

      const qty =
        targetLeg === leg && 'amount' in update ? adjust(update.amount, market.precision.amount) : snapshot.qty;
      if (!Number.isFinite(qty) || qty <= 0) {
        return rejectPreflight(`Bitunix ${prefix.toUpperCase()} quantity is invalid`);
      }

      payload[`${prefix}Qty`] = `${qty}`;
      payload[`${prefix}OrderType`] = snapshot.orderType === 'LIMIT' ? 'LIMIT' : 'MARKET';

      if (snapshot.orderType === 'LIMIT') {
        const orderPrice = adjust(snapshot.orderPrice, market.precision.price);
        if (!Number.isFinite(orderPrice) || orderPrice <= 0) {
          return rejectPreflight(`Bitunix ${prefix.toUpperCase()} LIMIT execution price is invalid`);
        }
        payload[`${prefix}OrderPrice`] = `${orderPrice}`;
      }
    };

    appendLeg(leg);
    const siblingLeg = leg === 'tp' ? 'sl' : 'tp';
    const siblingOrder = this.store.orders.find(
      (candidate) =>
        candidate.symbol === order.symbol &&
        candidate.type === (siblingLeg === 'tp' ? OrderType.TakeProfit : OrderType.StopLoss) &&
        this.isStandaloneTpslOrder(candidate) &&
        (this.getTpslRawOrderId(candidate) === rawOrderId ||
          (positionId !== undefined && positionId !== null && candidate.info?.positionId === positionId)),
    );
    if (siblingOrder) {
      appendLeg(siblingLeg, siblingOrder);
    }

    try {
      const endpoint = isPositionOrder ? ENDPOINTS.MODIFY_POSITION_TP_SL_ORDER : ENDPOINTS.MODIFY_TP_SL_ORDER;
      const response = await this.unlimitedXHR.post(endpoint, payload);
      const code = Number(response?.data?.code);
      if (code !== 0) {
        throw new Error(response?.data?.msg || `Bitunix TP/SL modify failed with code ${response?.data?.code}`);
      }

      return [order.id];
    } catch (err: any) {
      const message = err?.response?.data?.msg || err?.message || 'Bitunix TP/SL modify failed';
      this.emitAndLogError(message);
      throw new Error(message);
    }
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

    if (!isVirtualTpSl && this.isStandaloneTpslOrder(order)) {
      return this.updateStandaloneTpslOrder(order, update);
    }

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
    const hasRawInfo = this.isRecord(info);
    const rawOrderId = hasRawInfo ? info.orderId : undefined;
    const rawOrderType = hasRawInfo ? String(info.orderType || info.type || '').toUpperCase() : '';
    const hasReconciledOrderId = hasRawInfo
      ? rawOrderId !== undefined && rawOrderId !== null && rawOrderId !== '' && String(rawOrderId) === targetOrder.id
      : targetOrder.id !== '';
    const hasReconciledOrderType = hasRawInfo ? rawOrderType === 'LIMIT' : targetOrder.type === OrderType.Limit;
    if (!hasReconciledOrderId || !hasReconciledOrderType) {
      return rejectPreflight(
        `Bitunix cannot safely amend ${targetOrder.id}: exact pending LIMIT order identity is not reconciled`,
      );
    }

    if (hasRawInfo && (typeof info.reduceOnly !== 'boolean' || info.reduceOnly !== targetOrder.reduceOnly)) {
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
      symbol: targetOrder.symbol,
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
      const stopType = this.getBitunixTriggerType(
        prefix === 'tp' ? targetOrder.tpTriggerType : targetOrder.slTriggerType,
        info?.[stopTypeKey],
      );
      // The modify-order schema documents MARKET as the default when the
      // attached protection's execution type is omitted.
      const protectionOrderType = String(info?.[orderTypeKey] || 'MARKET').toUpperCase();

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

      // Bitunix documents the private WebSocket update as authoritative.
      // Its response may identify a replacement order rather than echoing the
      // submitted id, so return that id to the chart alias layer when present.
      const responseOrderId = response?.data?.data?.orderId;
      const acknowledgedParentId =
        responseOrderId === undefined || responseOrderId === null || responseOrderId === ''
          ? targetOrder.id
          : String(responseOrderId);
      // The API modifies the parent, but the caller is updating a child chart
      // row. Preserve that child identity so the optimistic order layer never
      // overlays a TP/SL row onto the regular parent order.
      return [isVirtualTpSl ? `${acknowledgedParentId}__${virtualTpslLeg}` : acknowledgedParentId];
    } catch (err: any) {
      const message = err?.response?.data?.msg || err?.message || 'Bitunix modify order failed';
      if (isVirtualTpSl && this.isUnsupportedOrderOperation(message)) {
        try {
          return await this.recreateAttachedTpslParentOrder(targetOrder, virtualTpslLeg, payload);
        } catch (fallbackErr: any) {
          const fallbackMessage =
            fallbackErr?.response?.data?.msg || fallbackErr?.message || 'Bitunix cancel/recreate fallback failed';
          this.emitAndLogError(fallbackMessage);
          throw new Error(fallbackMessage);
        }
      }
      this.emitAndLogError(message);
      throw new Error(message);
    }
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    try {
      await this.hydratePositionIdBeforeFormatting(opts);
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
    this.assertCallerClientOrderIds(orders);
    for (const order of orders) {
      await this.hydratePositionIdBeforeFormatting(order);
    }
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

  private hydratePositionIdBeforeFormatting = async (opts: PlaceOrderOpts) => {
    const needsPositionId =
      opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit || opts.reduceOnly === true;
    if (!needsPositionId) {
      return;
    }

    const isFullPositionTpsl =
      (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) && (!opts.amount || opts.amount === 0);
    const positionSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const cacheKey = `${opts.symbol}:${positionSide}`;
    const position = this.store.positions.find(
      (p) => p.symbol === opts.symbol && p.side === positionSide && p.contracts > 0,
    );

    if (!position) {
      return;
    }

    if (!position.info?.positionId && !this.positionIdCache.get(cacheKey)) {
      const freshPositions = await this.fetchPositions();
      this.store.update({ positions: freshPositions });
    }

    if (isFullPositionTpsl) {
      const freshOrders = await this.fetchOrders();
      this.store.update({ orders: freshOrders });
    }
  };

  private formatCreateOrder = (opts: PlaceOrderOpts) => {
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
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
      clientId: callerClientOrderId ?? 'ts_' + uuid(),
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
    this.assertSingleNativeClientOrderId(callerClientOrderId, payloads.length);

    // Cache TIF by clientId for REST API workaround
    if (req.clientId && opts.timeInForce) {
      this.orderTifCache.set(req.clientId, opts.timeInForce);
    }
    if (req.clientId) {
      this.orderReduceOnlyCache.set(req.clientId, reduceOnly);
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
