import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';
import type { ClientOrderLookup } from '../../../exchanges/clientOrderId';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import groupBy from 'lodash/groupBy';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';
import { forEachSeries } from 'p-iteration';

import { createApiWithSecondaryKey } from '../../../createApiWithSecondaryKey';
import {
  Balance,
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
  PublicTrade,
  Ticker,
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { incomeCategoryFromType } from '../../../utils/incomeCategoryFromType';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { normalizeExecutionPositionSide } from '../../../utils/normalizeExecutionPositionSide';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, subtract } from '../../../utils/safe-math';
import { splitTimeRange } from '../../../utils/splitTimeRange';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange, CallableMethodsInWorker } from '../../../exchanges/base';
import { orderCarriesClientOrderId, patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { paginateIncomeRange } from '../../../exchanges/income-paging';
import { createAPI, createPublicAPI } from './binance.api';
import { deriveBinanceClosedPositions } from './binance.closed-pnl';
import { mapBinanceHistoricalOrder } from './binance.order-history';
import {
  BinanceIncomeRecord,
  ENDPOINTS,
  GET_POSITION_RISK_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from '../../../exchanges/binance/binance.types';
import { markBinanceOrdersCanceled, USER_TRADES_MAX_WINDOW_MS } from './binance.utils';
import { BinancePrivateWebsocket } from './binance.ws-private';
import { BinancePublicWebsocket } from './binance.ws-public';

const BINANCE_ORDER_DOES_NOT_EXIST = -2013;

type BinanceResponseRecord = Record<string, any>;

const describeBinanceResponseShape = (payload: unknown): string => {
  if (Array.isArray(payload)) return `array(${payload.length})`;
  if (payload === null) return 'null';
  if (typeof payload === 'object') {
    const keys = Object.keys(payload as Record<string, unknown>).slice(0, 6);
    return keys.length ? `object(${keys.join(',')})` : 'object';
  }
  return typeof payload;
};

const getBinanceResponseMessage = (payload: unknown): string => {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return '';
  const record = payload as Record<string, unknown>;
  const message = record.msg || record.message || record.error;
  const code = record.code;
  const parts = [code !== undefined ? `code=${String(code)}` : '', message ? String(message) : ''].filter(Boolean);
  return parts.join(' ');
};

const requireBinanceArrayResponse = <T>(endpoint: string, payload: unknown): T[] => {
  if (Array.isArray(payload)) return payload as T[];

  const shape = describeBinanceResponseShape(payload);
  const message = getBinanceResponseMessage(payload);
  throw new Error(
    `Binance ${endpoint} response malformed: expected array, received ${shape}${message ? ` (${message})` : ''}`,
  );
};

const emitBinanceExchangeError = (exchange: BaseExchange, err: any) => {
  const message = err?.response?.data?.msg || err?.message;
  if (err instanceof Error) {
    exchange.emitAndLogError(err, message ? `Exchange Error: ${message}` : undefined);
    return;
  }

  exchange.emitAndLogError(message);
};

const PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS = 3_000;
const isBinancePositionModeAlreadySetError = (err: any) => {
  const code = err?.response?.data?.code;
  return code === -4059 || code === '-4059';
};

const ACCOUNT_RECONCILIATION_INTERVAL_MS = 25_000;
const DEGRADED_ACCOUNT_RECONCILIATION_INTERVAL_MS = 3_000;
const POSITION_RISK_RECONCILIATION_INTERVAL_MS = 75_000;
/**
 * Binance rejects an income query wider than 7 days, and caps a page at 1000 rows.
 *
 * Both are venue limits, not tuning knobs. The pager does not depend on either being exact:
 * it walks until the returned records span the requested window, so a narrower real window
 * costs extra requests rather than silently losing rows.
 */
const INCOME_HISTORY_MAX_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;
const INCOME_HISTORY_PAGE_LIMIT = 1000;

/**
 * Pacing between seven-day chunks. This is a WEIGHT budget, not politeness.
 *
 * `/fapi/v1/income` costs **weight 30** against binance's 2400/min IP budget — 80 requests
 * per minute. The client's own `maxRPS: 3` limiter counts REQUESTS, not weight, so it would
 * permit 5400 weight/min on this endpoint. Removing this sleep on the grounds that "the
 * axios rate limiter already paces us" is therefore wrong, and `binance.income-pacing.test`
 * exists to catch exactly that.
 */
const INCOME_HISTORY_CHUNK_DELAY_MS = 2_000;
const REQUEST_WEIGHT_LIMIT_PER_MINUTE = 2_400;
const REQUEST_WEIGHT_BACKOFF_THRESHOLD = REQUEST_WEIGHT_LIMIT_PER_MINUTE * 0.8;
const REQUEST_WEIGHT_BACKOFF_MS = 60_000;

// TODO: translate our err messages
export class BinanceExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Binance', /^[.A-Z:/a-z0-9_-]{1,36}$/, {
    mint: () => 'x-s3vFFRQp' + uuid(),
  });

  // Query order sees filled and cancelled orders too, so Binance can say an ID was never placed.
  findOrderByClientOrderId = async (symbol: string, clientOrderId: string): Promise<ClientOrderLookup> => {
    const inStore = this.store.orders.find(
      (order) => order.symbol === symbol && orderCarriesClientOrderId(order, clientOrderId),
    );
    // Secondary GET snapshots may belong to another account; their cache cannot prove primary identity.
    if (inStore && !this.options.secondary_key) return { found: true, order: inStore };
    if (!this.store.marketsBySymbol[symbol]) return { found: false, definitive: false };

    try {
      // The primary key placed it; a read-only secondary key may belong to another account.
      // `symbol` as placed (BTCUSDT) — Binance's market.id is BTC/USDT:USDT.
      const { data } = await (this._primaryXhr ?? this.xhr).get<Record<string, any>>(ENDPOINTS.ORDER, {
        params: { symbol, origClientOrderId: clientOrderId },
      });
      const order = mapBinanceHistoricalOrder(data ?? {});
      if (!order) return { found: false, definitive: false };
      return {
        found: true,
        order: {
          id: order.id,
          symbol,
          side: order.side,
          type: order.type,
          status: order.status,
          price: order.price ?? 0,
          amount: order.amount,
          filled: order.filled,
          remaining: Math.max(order.amount - order.filled, 0),
          reduceOnly: order.reduceOnly ?? false,
          lastUpdate: order.updatedTime ?? order.createdTime,
          info: data,
        },
      };
    } catch (error: any) {
      if (error?.response?.data?.code === BINANCE_ORDER_DOES_NOT_EXIST) return { found: false, definitive: true };
      throw error;
    }
  };

  /** Journal records: this venue has no reported closed axis, so round trips fold from fills. */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) =>
    this.foldClosedPositionRecords(opts, { symbolScoped: true });

  private hasLoggedBalanceAggregation = false;
  private positionStateVersion = 0;
  private lastAccountReconciliationAt = 0;
  private lastPositionRiskReconciliationAt = 0;
  private backgroundReconciliationPausedUntil = 0;

  markPositionStateUpdatedByWebsocket = () => {
    this.positionStateVersion += 1;
  };

  getPositionStateVersion = () => this.positionStateVersion;

  // Helper to detect if availableBalance is 0 but user has stablecoin balances
  detectBalanceIssue(_accountData: any): boolean {
    // This method is deprecated since we now always aggregate from individual assets
    // Keeping it for backwards compatibility but it always returns false
    return false;
  }
  xhr: Axios;
  _primaryXhr: Axios;
  _secondaryXhr: Axios;
  unlimitedXHR: Axios;
  publicXhr: Axios;

  publicWebsocket: BinancePublicWebsocket;
  privateWebsocket: BinancePrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    const [xhr, primaryXhr, secondaryXhr] = createApiWithSecondaryKey(opts, this, createAPI, {
      rateLimit: (xhr) =>
        rateLimit(xhr, {
          maxRPS: 3,
        }),
      // secondaryRateLimit: (xhr) =>
      //   rateLimit(xhr, {
      //     maxRPS: 3,
      //   }),
    });

    this.xhr = xhr;
    this._primaryXhr = primaryXhr;
    this._secondaryXhr = secondaryXhr;

    this.publicXhr = createPublicAPI(opts, this);
    this.unlimitedXHR = createAPI(opts, this);
    this.installRequestWeightGuards([primaryXhr, secondaryXhr, this.publicXhr, this.unlimitedXHR]);

    this.publicWebsocket = new BinancePublicWebsocket(this);
    this.privateWebsocket = new BinancePrivateWebsocket(this);
  }

  private getResponseHeader = (headers: any, name: string): unknown => {
    if (!headers) return undefined;
    if (typeof headers.get === 'function') return headers.get(name);
    return headers[name] ?? headers[name.toLowerCase()] ?? headers[name.toUpperCase()];
  };

  private pauseBackgroundReconciliation = (durationMs: number) => {
    this.backgroundReconciliationPausedUntil = Math.max(
      this.backgroundReconciliationPausedUntil,
      Date.now() + durationMs,
    );
  };

  private observeRequestWeight = (response: any) => {
    const status = response?.status;
    if (status === 429 || status === 418) {
      const retryAfterSeconds = Number(this.getResponseHeader(response?.headers, 'retry-after'));
      this.pauseBackgroundReconciliation(
        Number.isFinite(retryAfterSeconds) && retryAfterSeconds > 0
          ? retryAfterSeconds * 1_000
          : REQUEST_WEIGHT_BACKOFF_MS,
      );
      return;
    }

    const usedWeight = Number(this.getResponseHeader(response?.headers, 'x-mbx-used-weight-1m'));
    if (Number.isFinite(usedWeight) && usedWeight >= REQUEST_WEIGHT_BACKOFF_THRESHOLD) {
      this.pauseBackgroundReconciliation(REQUEST_WEIGHT_BACKOFF_MS);
    }
  };

  private installRequestWeightGuards = (clients: Axios[]) => {
    new Set(clients).forEach((client) => {
      client.interceptors.response.use(
        (response) => {
          this.observeRequestWeight(response);
          return response;
        },
        (error) => {
          this.observeRequestWeight(error?.response);
          return Promise.reject(error);
        },
      );
    });
  };

  isBackgroundReconciliationPaused = () => Date.now() < this.backgroundReconciliationPausedUntil;

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  getAccount = async () => {
    const {
      data: [{ accountAlias }],
    } = await this.xhr.get(ENDPOINTS.BALANCE);

    return { userId: accountAlias };
  };

  validateAccount = async () => {
    let balance: Balance | undefined;
    try {
      const { data } = await this._primaryXhr.get(ENDPOINTS.ACCOUNT);
      // /fapi/v3/account reports USD totals at the top level. Good enough for the
      // new-account balance gate; full per-asset aggregation needs live tickers.
      const total = parseFloat(data?.totalWalletBalance);
      if (Number.isFinite(total)) {
        const free = parseFloat(data?.availableBalance) || 0;
        const upnl = parseFloat(data?.totalUnrealizedProfit) || 0;
        balance = { total, free, used: total - free, upnl };
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }

    if (this.options.secondary_key) {
      try {
        await this._secondaryXhr.get(ENDPOINTS.ACCOUNT);
      } catch (err: any) {
        return { error: `Secondary Key: ${err?.response?.data?.msg || err?.message}` };
      }
    }
    return { error: '', balance };
  };

  // The shared public exchange has no keys, so its markets carry leverage.max = 1.
  private loadAccountMaxLeverage = () =>
    this.loadAccountMarketOverlay(async () => {
      const { data } = await this.xhr.get<unknown>(ENDPOINTS.LEVERAGE_BRACKET);
      const brackets = requireBinanceArrayResponse<Record<string, any>>(ENDPOINTS.LEVERAGE_BRACKET, data);
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

  tick = async (_count = 0) => {
    if (!this.isDisposed) {
      const now = Date.now();
      const accountReconciliationInterval = this.privateWebsocket.isConnected
        ? ACCOUNT_RECONCILIATION_INTERVAL_MS
        : DEGRADED_ACCOUNT_RECONCILIATION_INTERVAL_MS;
      const shouldReconcilePositionRisk =
        !this.isBackgroundReconciliationPaused() &&
        (this.lastPositionRiskReconciliationAt === 0 ||
          now - this.lastPositionRiskReconciliationAt >= POSITION_RISK_RECONCILIATION_INTERVAL_MS);
      const shouldReconcileAccount =
        !this.isBackgroundReconciliationPaused() &&
        (this.lastAccountReconciliationAt === 0 ||
          now - this.lastAccountReconciliationAt >= accountReconciliationInterval);

      try {
        if (this.ordersSnapshotStale) await this.retryOrdersSnapshot();
        if (this.isDisposed) return;
        if (shouldReconcilePositionRisk) {
          const positionStateVersion = this.getPositionStateVersion();
          const positionsWithRisk = await this.fetchPositions();
          if (positionStateVersion === this.getPositionStateVersion()) {
            this.store.update({ positions: positionsWithRisk });
          }
          this.lastPositionRiskReconciliationAt = Date.now();
          if (this.isDisposed) return;
        }
        if (shouldReconcileAccount && !this.isBackgroundReconciliationPaused()) {
          const positionStateVersion = this.getPositionStateVersion();
          const { balance, positions } = await this.fetchPositionsAndBalance();
          if (this.isDisposed) return;

          this.store.update({
            balance,
            ...(positionStateVersion === this.getPositionStateVersion() && { positions }),
            loaded: {
              ...this.store.loaded,
              balance: true,
              positions: true,
            },
          });
          this.lastAccountReconciliationAt = Date.now();
        }
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS);
      }
    }
  };

  fetchMarkets = async () => {
    try {
      const { data: marketData } = await this.publicXhr.get<{ symbols?: unknown }>(ENDPOINTS.MARKETS);
      const symbols = requireBinanceArrayResponse<BinanceResponseRecord>(
        `${ENDPOINTS.MARKETS}.symbols`,
        marketData?.symbols,
      );

      const { data: leverageBracketData } = this.isPublicDataOnly
        ? { data: [] }
        : await this.xhr.get<unknown>(ENDPOINTS.LEVERAGE_BRACKET);
      const leverageBrackets = requireBinanceArrayResponse<BinanceResponseRecord>(
        ENDPOINTS.LEVERAGE_BRACKET,
        leverageBracketData,
      );

      const markets: Market[] = symbols
        .filter(
          (m) =>
            ['PERPETUAL', 'TRADIFI_PERPETUAL'].includes(v(m, 'contractType')) &&
            ['USDT', 'USDC'].includes(v(m, 'marginAsset')),
        )
        .map((m) => {
          const filters = Array.isArray(m.filters) ? m.filters : [];
          const p = filters.find((f: any) => v(f, 'filterType') === 'PRICE_FILTER');

          const amt = filters.find((f: any) => v(f, 'filterType') === 'LOT_SIZE');

          const mAmt = filters.find((f: any) => v(f, 'filterType') === 'MARKET_LOT_SIZE');
          const brackets = leverageBrackets.find((b) => b.symbol === m.symbol)?.brackets;
          const baseAsset = v(m, 'baseAsset');
          const quoteAsset = v(m, 'quoteAsset');
          const marginAsset = v(m, 'marginAsset');

          return {
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
          };
        });

      return markets;
    } catch (err: any) {
      emitBinanceExchangeError(this, err);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    try {
      const { data: dailyData } = await this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_24H);
      const dailys = requireBinanceArrayResponse<BinanceResponseRecord>(ENDPOINTS.TICKERS_24H, dailyData);

      const { data: bookData } = await this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_BOOK);
      const books = requireBinanceArrayResponse<BinanceResponseRecord>(ENDPOINTS.TICKERS_BOOK, bookData);

      const { data: priceData } = await this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_PRICE);
      const prices = requireBinanceArrayResponse<BinanceResponseRecord>(ENDPOINTS.TICKERS_PRICE, priceData);

      // Use O(1) hashmap lookup instead of O(n) find
      const marketsBySymbol = this.store.marketsBySymbol;
      return books.reduce((acc: Ticker[], book) => {
        const market = marketsBySymbol[book.symbol];

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
      emitBinanceExchangeError(this, err);
      return this.store.tickers;
    }
  };

  fetchPositions = async () => {
    // const { positions } = await this.fetchPositionsAndBalance();
    // return positions;
    try {
      const [{ data }, { data: symbolConfigs }] = await Promise.all([
        this.xhr.get<GET_POSITION_RISK_RESPONSE>(ENDPOINTS.POSITION_RISK),
        this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.SYMBOL_CONFIG),
      ]);
      const configBySymbol = Object.fromEntries(symbolConfigs.map((config) => [config.symbol, config]));
      const formattedPositions = data.map((p) => {
        const pSide = p.positionSide;
        const contracts = parseFloat(p.positionAmt);
        const side =
          (pSide in POSITION_SIDE && POSITION_SIDE[pSide]) || (contracts > 0 ? PositionSide.Long : PositionSide.Short);
        const positionMode = p.positionSide === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        const config = configBySymbol[p.symbol];
        const marginType = config?.marginType === 'ISOLATED' ? MarginType.Isolated : MarginType.Cross;
        const notional = Math.abs(parseFloat(p.notional));
        const leverage = parseFloat(config?.leverage || '0');
        const isolatedMargin = parseFloat(v(p, 'isolatedMargin') || '0');
        const marginSize =
          isolatedMargin > 0 ? isolatedMargin : leverage > 0 ? Math.abs(notional) / leverage : undefined;
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
          marginSize,
          marginType,
          positionMode,
          info: p,
        };
        return res;
      });

      const mode = this.store.options.isHedged ? PositionMode.Hedged : PositionMode.OneWay;
      const fakePositions = this.store.markets.flatMap((market): Position[] =>
        [PositionSide.Long, PositionSide.Short].map((side) => {
          const config = configBySymbol[market.symbol];
          const leverage = parseFloat(config?.leverage || '0');
          return {
            id: createPositionId(market.symbol, side, mode),
            symbol: market.symbol,
            side,
            entryPrice: 0,
            notional: 0,
            leverage,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            marginSize: 0,
            marginType: config?.marginType === 'ISOLATED' ? MarginType.Isolated : MarginType.Cross,
            positionMode: mode,
            info: { synthetic: true, symbolConfig: config },
          };
        }),
      );

      return uniqBy([...formattedPositions, ...fakePositions], 'id');
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.positions;
    }
  };

  fetchPositionsAndBalance = async () => {
    try {
      const { data } = await this.xhr.get<Record<string, any>>(ENDPOINTS.ACCOUNT);
      //
      // {
      //   "symbol": "FETUSDT",
      //   "initialMargin": "0",
      //   "maintMargin": "0",
      //   "unrealizedProfit": "0.00000000",
      //   "positionInitialMargin": "0",
      //   "openOrderInitialMargin": "0",
      //   "leverage": "20",
      //   "isolated": false,
      //   "entryPrice": "0.0",
      //   "breakEvenPrice": "0.0",
      //   "maxNotional": "25000",
      //   "positionSide": "BOTH",
      //   "positionAmt": "0",
      //   "notional": "0",
      //   "isolatedWallet": "0",
      //   "updateTime": 0,
      //   "bidNotional": "0",
      //   "askNotional": "0"
      // }

      // Define list of stablecoins to aggregate
      const STABLECOINS = ['USDT', 'USDC', 'BUSD', 'FDUSD', 'BFUSD', 'LDUSDT', 'BNFCR'];

      // Initialize balance values
      let total = 0;
      let free = 0;
      let upnl = 0;
      let used = 0;

      // Track details for logging
      const balanceDetails: {
        asset: string;
        available: number;
        wallet: number;
        initial: number;
        unrealized: number;
        usdValue?: number;
        price?: number;
      }[] = [];

      try {
        for (const asset of data?.assets || []) {
          const assetWallet = parseFloat(asset.walletBalance || '0');
          const assetAvailable = parseFloat(asset.availableBalance || '0');
          const assetInitial = parseFloat(asset.initialMargin || '0');
          const assetUnrealized = parseFloat(asset.unrealizedProfit || '0');

          // Skip assets with no balance
          if (assetWallet === 0 && assetAvailable === 0 && assetInitial === 0 && Math.abs(assetUnrealized) === 0) {
            continue;
          }

          if (STABLECOINS.includes(asset.asset)) {
            // Direct aggregation for stablecoins (1:1 USD value)
            total += assetWallet;
            free += assetAvailable;
            used += assetInitial;
            upnl += assetUnrealized;

            balanceDetails.push({
              asset: asset.asset,
              available: assetAvailable,
              wallet: assetWallet,
              initial: assetInitial,
              unrealized: assetUnrealized,
              usdValue: assetWallet,
            });
          } else {
            // For non-stablecoins, only look for price if there's actual balance
            let usdPrice = 0;
            let marketSymbol = '';

            // Only search for markets if the asset has some balance
            if (assetWallet > 0 || assetAvailable > 0 || assetInitial > 0 || Math.abs(assetUnrealized) > 0) {
              // First try to find a direct USDT pair
              const directMarket = this.store.markets.find(
                (m) => m.base === asset.asset && (m.quote === 'USDT' || m.quote === 'USDC' || m.quote === 'BUSD'),
              );

              if (directMarket) {
                marketSymbol = directMarket.symbol;
                // Use O(1) hashmap lookup instead of O(n) find
                const ticker = this.store.tickersBySymbol[marketSymbol];
                if (ticker && ticker.last > 0) {
                  usdPrice = ticker.last;
                }
              }

              // If no direct pair, try common routing pairs (e.g., XXXBTC -> BTCUSDT)
              if (usdPrice === 0 && asset.asset !== 'BTC') {
                const btcMarket = this.store.markets.find((m) => m.base === asset.asset && m.quote === 'BTC');
                if (btcMarket) {
                  // Use O(1) hashmap lookup instead of O(n) find
                  const btcTicker = this.store.tickersBySymbol[btcMarket.symbol];
                  const btcUsdtTicker = this.store.tickersBySymbol['BTCUSDT'] || this.store.tickersBySymbol['BTCUSDC'];

                  if (btcTicker && btcTicker.last > 0 && btcUsdtTicker && btcUsdtTicker.last > 0) {
                    usdPrice = btcTicker.last * btcUsdtTicker.last;
                    marketSymbol = `${btcMarket.symbol} -> ${btcUsdtTicker.symbol}`;
                  }
                }
              }
            }

            // If we have balance and found a price, add USD value to totals
            if (usdPrice > 0) {
              const walletUsdValue = assetWallet * usdPrice;
              const availableUsdValue = assetAvailable * usdPrice;
              const initialUsdValue = assetInitial * usdPrice;
              const unrealizedUsdValue = assetUnrealized * usdPrice;

              total += walletUsdValue;
              free += availableUsdValue;
              used += initialUsdValue;
              upnl += unrealizedUsdValue;

              balanceDetails.push({
                asset: asset.asset,
                available: assetAvailable,
                wallet: assetWallet,
                initial: assetInitial,
                unrealized: assetUnrealized,
                usdValue: walletUsdValue,
                price: usdPrice,
              });
            } else if (assetWallet > 0 || assetAvailable > 0 || assetInitial > 0 || Math.abs(assetUnrealized) > 0) {
              // Has balance but no price found
              balanceDetails.push({
                asset: asset.asset,
                available: assetAvailable,
                wallet: assetWallet,
                initial: assetInitial,
                unrealized: assetUnrealized,
                usdValue: 0,
                price: 0,
              });
            }
            // If no balance at all, we skip it entirely (already handled by the continue statement above)
          }
        }

        // Log balance aggregation details periodically
        if (!this.hasLoggedBalanceAggregation && balanceDetails.length > 0) {
          this.hasLoggedBalanceAggregation = true;

          const stablecoins = balanceDetails.filter((d) => STABLECOINS.includes(d.asset) && d.wallet > 0);
          const nonStablecoins = balanceDetails.filter((d) => !STABLECOINS.includes(d.asset) && d.wallet > 0);

          let logMessage = `Balance aggregation:\n`;

          if (stablecoins.length > 0) {
            logMessage += `  Stablecoins: ${stablecoins.map((d) => `${d.asset}(${d.wallet})`).join(', ')}\n`;
          }

          if (nonStablecoins.length > 0) {
            logMessage += `  Non-stablecoins: ${nonStablecoins
              .map((d) => `${d.asset}(${d.wallet} @ $${d.price?.toFixed(2)} = $${d.usdValue?.toFixed(2)})`)
              .join(', ')}\n`;
          }

          logMessage += `  Total: $${total.toFixed(2)}, Free: $${free.toFixed(2)}, Used: $${used.toFixed(2)}, UPnL: $${upnl.toFixed(2)}`;

          this.log(logMessage, LogSeverity.Info);
        }
      } catch (e) {
        this.log(`Error aggregating balances: ${e}`, LogSeverity.Error);
        // Fallback to top-level values if available
        total = parseFloat(data.totalWalletBalance || '0');
        free = parseFloat(data.availableBalance || '0');
        upnl = parseFloat(data.totalUnrealizedProfit || '0');
        used = parseFloat(data.totalInitialMargin || '0');
      }

      const { positions: _positions, ...rest } = data;
      const balance = { total, free, used, upnl, info: rest };

      // Account V3's compact position rows omit entry price, leverage,
      // liquidation price, and margin type. The dedicated V3 position-risk +
      // symbol-config reconciliation owns positions; account reconciliation
      // only refreshes the balance and preserves that richer snapshot.
      return { balance, positions: this.store.positions };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { balance: this.store.balance, positions: this.store.positions };
    }
  };

  fetchOrders = async () => {
    try {
      // Fetch both regular orders and algo orders in parallel
      const [regularResponse, algoResponse] = await Promise.all([
        this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.OPEN_ORDERS),
        this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.OPEN_ALGO_ORDERS, {
          params: { algoType: 'CONDITIONAL' },
        }),
      ]);

      const regularData = regularResponse.data;
      const algoData = algoResponse.data;

      // Log the response if it's not an array
      if (!Array.isArray(regularData)) {
        console.error('[Binance] fetchOrders received non-array response:', regularData);
        this.log(`fetchOrders received non-array response: ${JSON.stringify(regularData)}`, LogSeverity.Error);
      }

      // Ensure data is an array (in case API returns something else when no orders)
      const ordersData = Array.isArray(regularData) ? regularData : [];

      const orders: Order[] = ordersData.map((o) => {
        return {
          id: v(o, 'orderId')?.toString(),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: ORDER_TYPE[o.type],
          timeInForce: TIME_IN_FORCE[o.timeInForce],
          side: ORDER_SIDE[o.side],
          price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')),
          amount: parseFloat(v(o, 'origQty')),
          reduceOnly: v(o, 'reduceOnly') || false,
          close: v(o, 'closePosition') || false,
          filled: parseFloat(v(o, 'executedQty')),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: subtract(v(o, 'origQty'), v(o, 'executedQty')),
          info: o,
          ...(ORDER_TYPE[o.type] === OrderType.TakeProfit && {
            tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')],
          }),
          ...(ORDER_TYPE[o.type] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
        };
      });

      // Parse algo orders (stop/TP/trailing orders) - response is a direct array
      const algoOrdersData = Array.isArray(algoData) ? algoData : [];
      const algoOrders: Order[] = algoOrdersData.map((o: Record<string, any>) => {
        const orderType = ORDER_TYPE[v(o, 'orderType')];
        // Always use algoId (Binance's order ID) - clientAlgoId from website-placed orders is unusable
        const algoId = v(o, 'algoId');
        return {
          id: algoId?.toString(),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: orderType,
          side: ORDER_SIDE[o.side],
          price: parseFloat(v(o, 'triggerPrice')) || 0,
          amount: parseFloat(v(o, 'quantity')) || 0,
          reduceOnly: v(o, 'reduceOnly') || false,
          close: v(o, 'closePosition') || false,
          filled: 0,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(v(o, 'quantity')) || 0,
          // Explicitly include algoId for consistent detection in cancelOrders
          // (matches WebSocket handler format for algo order identification)
          info: { algoId, ...o },
          ...(orderType === OrderType.TakeProfit && {
            tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')],
          }),
          ...(orderType === OrderType.StopLoss && {
            slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')],
          }),
        };
      });

      this.ordersSnapshotStale = false;
      return [...orders, ...algoOrders];
    } catch (error: any) {
      console.error('[Binance] fetchOrders error:', error);
      console.error('[Binance] fetchOrders error response:', error?.response?.data);
      this.log(`fetchOrders error: ${error?.message || error}`, LogSeverity.Error);

      // A failed full snapshot is not authoritative. Preserve the current
      // store state so startup, reconnect, and throttled refresh callers
      // cannot erase live regular or algo orders on transient failures.
      this.ordersSnapshotStale = true;
      return this.store.orders;
    }
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 1000 } = opts;

    if (!symbol) {
      // Binance requires symbol for userTrades queries
      return [];
    }

    try {
      const params: Record<string, any> = {
        symbol,
        limit: Math.min(limit, 1000),
      };

      if (orderId) {
        params.orderId = orderId;
      }

      // `/fapi/v1/userTrades` REJECTS a window wider than 7 days — it does not clamp it.
      // The catch below turns that error into `[]`, so an unchunked 30-day ask returned
      // ZERO fills and read downstream as "this symbol has never traded". Same defect as
      // `deriveClosedPositions` was already written to avoid, in this same class.
      const windows =
        from && to ? splitTimeRange(from, to, USER_TRADES_MAX_WINDOW_MS) : [{ startTime: from, endTime: to }];

      const rows: any[] = [];
      for (const window of windows) {
        const { data } = await this.xhr.get<any[]>(ENDPOINTS.USER_TRADES, {
          params: omitUndefined({ ...params, startTime: window.startTime, endTime: window.endTime }),
        });
        if (Array.isArray(data)) rows.push(...data);
      }

      const trades = rows;

      return trades.map((entry) =>
        // omitUndefined for parity with every other venue's executions mapper: an
        // explicit `undefined` key survives structuredClone across the worker
        // boundary, where an omitted one does not.
        omitUndefined({
          // Per-fill trade id — the only idempotent dedupe key. orderId is
          // shared across the partial fills of one order.
          id: entry.id !== undefined && entry.id !== null ? String(entry.id) : undefined,
          timestamp: Math.floor(entry.time / 1000), // Convert from milliseconds to seconds
          symbol: entry.symbol,
          orderId: entry.orderId?.toString(),
          price: parseFloat(entry.price),
          qty: parseFloat(entry.qty),
          side: ORDER_SIDE[entry.side] || OrderSide.Buy,
          commission: parseFloat(entry.commission || '0'),
          commissionAsset: entry.commissionAsset,
          positionSide: entry.positionSide ? normalizeExecutionPositionSide(entry.positionSide) : undefined,
          realizedPnl: Number.isFinite(parseFloat(entry.realizedPnl)) ? parseFloat(entry.realizedPnl) : undefined,
          maker: typeof entry.maker === 'boolean' ? entry.maker : undefined,
        }),
      );
    } catch (error: any) {
      this.log(`Error fetching executions: ${error.message}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Fetch income history from Binance Futures.
   * Returns realized PnL, funding fees, commissions, etc.
   *
   * Rate limit: 30 requests per minute
   * Max: 1000 records per request, up to 7 days per request
   *
   * @param opts - Options for filtering income history
   * @returns Array of normalized income records
   */
  /**
   * Binance has no closed-position endpoint, so this folds `/fapi/v1/userTrades`. That is
   * a DIFFERENT source from the `/fapi/v1/income` the ledger reads, which is why the flag
   * below says it does not share a source — a fold of a different endpoint is still a
   * legitimate cross-check. See `binance.closed-pnl.ts`.
   */
  closedPositionsShareLedgerSource = false;

  /** Historical orders, including ones that never filled — see binance.order-history.ts. */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly || !opts?.symbol) {
      // `/fapi/v1/allOrders` requires a symbol, unlike bybit's account-wide history.
      return [];
    }

    try {
      const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.ALL_ORDERS, {
        params: omitUndefined({
          symbol: opts.symbol,
          startTime: opts.from,
          endTime: opts.to,
          limit: Math.min(opts.limit || 1000, 1000),
        }),
      });

      const rows = Array.isArray(data) ? data : [];
      return rows
        .map(mapBinanceHistoricalOrder)
        .filter((order): order is HistoricalOrder => order !== null)
        .sort((a, b) => b.createdTime - a.createdTime);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  deriveClosedPositions = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    const to = opts.to ?? Date.now();
    const from = opts.from ?? to - 30 * 24 * 60 * 60 * 1000;

    try {
      const rows: Array<Record<string, any>> = [];

      // `/fapi/v1/userTrades` rejects a window wider than 7 days outright. A 30-day ask
      // returns an error that the catch below turns into an empty list — which reads
      // downstream as "this symbol has no closes", not as a failed request. This loop was
      // the only correct copy in the class; the two sibling methods lacked it entirely,
      // which is why the split now lives in one shared, tested helper.
      for (const window of splitTimeRange(from, to, USER_TRADES_MAX_WINDOW_MS)) {
        const params = omitUndefined({
          symbol: opts.symbol,
          startTime: window.startTime,
          endTime: window.endTime,
          limit: 1000,
        });
        const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.USER_TRADES, { params });
        if (Array.isArray(data)) rows.push(...data);
      }

      return deriveBinanceClosedPositions(rows, opts.accountName || '').sort((a, b) => b.closedTime - a.closedTime);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  /**
   * One income page, RETHROWING on failure.
   *
   * `fetchIncomeHistory` below swallows errors into `[]` and must keep doing so — the worker
   * `CallMethodHandler` and the module-frame account hooks both call it directly and expect
   * an array. But a pager cannot be handed that contract: a rate-limited page and an empty
   * window are then the same value, so a 429 mid-walk reads as "nothing more here" and
   * silently truncates the ledger.
   *
   * That is not hypothetical. The live paging probe hit binance's 2400/min IP limit — the
   * proxy IP is shared — and the walk returned 492 of 512 records while reporting success,
   * because 20 rows sat behind pages that errored into `[]`. An error becoming authoritative
   * empty data is the exact thing `packages/safe-cex/CLAUDE.md` forbids, one axis over from
   * the fabricated zero PnL.
   */
  private _fetchIncomeHistoryPage = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) return [];

    const params: Record<string, any> = {
      limit: Math.min(opts?.limit || 1000, 1000),
    };
    if (opts?.symbol) params.symbol = opts.symbol;
    if (opts?.incomeType) params.incomeType = opts.incomeType;
    if (opts?.startTime) params.startTime = opts.startTime;
    if (opts?.endTime) params.endTime = opts.endTime;

    const { data } = await this.xhr.get<BinanceIncomeRecord[]>(ENDPOINTS.INCOME, { params });
    const records = Array.isArray(data) ? data : [];

    return records.map((record) => ({
      symbol: record.symbol,
      incomeType: record.incomeType,
      income: parseFloat(record.income),
      asset: record.asset,
      timestamp: record.time,
      tranId: record.tranId,
      tradeId: record.tradeId,
      category: incomeCategoryFromType(record.incomeType),
    }));
  };

  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    try {
      const params: Record<string, any> = {
        limit: Math.min(opts?.limit || 1000, 1000),
      };

      if (opts?.symbol) {
        params.symbol = opts.symbol;
      }
      if (opts?.incomeType) {
        params.incomeType = opts.incomeType;
      }
      if (opts?.startTime) {
        params.startTime = opts.startTime;
      }
      if (opts?.endTime) {
        params.endTime = opts.endTime;
      }

      const { data } = await this.xhr.get<BinanceIncomeRecord[]>(ENDPOINTS.INCOME, { params });

      // Ensure data is an array before mapping
      const records = Array.isArray(data) ? data : [];

      return records.map((record) => ({
        symbol: record.symbol,
        incomeType: record.incomeType,
        income: parseFloat(record.income),
        asset: record.asset,
        timestamp: record.time,
        tranId: record.tranId,
        tradeId: record.tradeId,
        category: incomeCategoryFromType(record.incomeType),
      }));
    } catch (error: any) {
      this.log(`Error fetching income history: ${error.message}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Income over a range. Binance caps a single income query at 7 days, so the range is
   * chunked; within a chunk it is paged until the records SPAN the window.
   *
   * This used to page off a full-page count at BOTH ends of the loop — `length === 1000` to
   * enter the inner walk, `length < 1000` to leave it. Both are guesses about the venue:
   * a server-side filter, or any cap below the asked limit, returns a short page with more
   * behind it and the walk stops early, silently. That is worse here than on weex, because
   * binance currently PASSES the canonical gate — a silent truncation corrupts a green
   * verdict rather than producing an obviously absent one. It has never fired only because
   * the test account is quiet enough that no 7-day window reaches 1000 records.
   *
   * The old inner cursor also stepped `lastTimestamp + 1`, which assumes the last row is the
   * newest (ascending order) AND drops any rows sharing that millisecond past the page
   * boundary. The shared pager assumes no order and reports the saturated-millisecond case
   * instead of silently skipping it. See `exchanges/income-paging.ts`.
   */
  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    const { records, truncated, exhaustedChunks } = await paginateIncomeRange(
      // The RETHROWING variant — see `_fetchIncomeHistoryPage`. Handing the pager the
      // swallowing one turns a 429 into "no more records" and truncates silently.
      ({ startTime: from, endTime: to, limit }) =>
        this._fetchIncomeHistoryPage({ startTime: from, endTime: to, incomeType: incomeType as any, limit }),
      {
        startTime,
        endTime,
        chunkMs: INCOME_HISTORY_MAX_WINDOW_MS,
        pageLimit: INCOME_HISTORY_PAGE_LIMIT,
        // Weight budget, not politeness — see the constant. `this.xhr`'s maxRPS 3 does NOT
        // substitute for it: at weight 30 that is 5400 weight/min against a 2400 cap.
        chunkDelayMs: INCOME_HISTORY_CHUNK_DELAY_MS,
      },
    );

    // A partial ledger reconciles perfectly well against itself, so an unannounced one is
    // worse than an error. Callers cannot distinguish a truncated walk from a complete one.
    if (truncated) {
      this.log(
        `Income history for ${new Date(startTime).toISOString()}..${new Date(endTime).toISOString()} is INCOMPLETE — ` +
          `${exhaustedChunks} chunk(s) could not be walked to completion. Any total derived from it is a lower bound.`,
        LogSeverity.Warning,
      );
    }

    return records;
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const limit = Math.min(opts.limit || 500, 1500);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.publicXhr.get<any[][]>(ENDPOINTS.KLINE, {
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

    if (!Array.isArray(data)) {
      return [];
    }

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
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeCandle(opts, callback);
    }

    let disposed = false;
    const unsubscribe = this.publicWebsocket.listenOHLCV(opts, callback);

    // Binance kline WS normally updates every 250ms, but the first pushed
    // frame can lag after a fresh subscribe. Seed the realtime path so the
    // chart current candle/countdown is live immediately after history loads.
    void this.fetchOHLCV({ ...opts, limit: 1 })
      .then((candles) => {
        if (disposed || this.isDisposed) return;
        const latest = candles[candles.length - 1];
        if (latest) {
          callback(latest);
        }
      })
      .catch((err: any) => {
        this.log(`Unable to seed Binance live candle: ${err?.message || err}`, LogSeverity.Debug);
      });

    return () => {
      disposed = true;
      unsubscribe();
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeOrderbook(symbol, callback);
    }

    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchPublicTrades = async (symbol: string, limit?: number): Promise<PublicTrade[]> => {
    // Binance max limit is 1000, default is 500
    const maxLimit = 1000;
    const defaultLimit = 1000;
    const actualLimit = limit ? Math.min(limit, maxLimit) : defaultLimit;

    const { data } = await this.publicXhr.get(ENDPOINTS.TRADES, {
      params: {
        symbol,
        limit: actualLimit,
      },
    });

    // publicXhr skips the negative-`code` rejection interceptor that
    // `xhr` has (see createPublicAPI in binance.api.ts), so an unknown
    // symbol returns a 200 with `{ code, msg }` instead of throwing.
    if (!Array.isArray(data)) {
      return [];
    }

    return data.map((trade: any): PublicTrade => ({
      id: trade.id.toString(),
      symbol,
      price: parseFloat(trade.price),
      qty: parseFloat(trade.qty),
      quoteQty: parseFloat(trade.quoteQty),
      timestamp: trade.time,
      side: trade.isBuyerMaker ? OrderSide.Sell : OrderSide.Buy, // isBuyerMaker means sell order was in the book (taker was buyer)
    }));
  };

  fetchPositionMode = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.HEDGE_MODE);
    return data.dualSidePosition === true;
  };

  changePositionMode = async (hedged: boolean, symbol?: string) => {
    if (this.store.positions.filter((p) => p.contracts > 0).length > 0) {
      this.emitAndLogError('Please close all positions before switching position mode');
      return;
    }

    try {
      await this.xhr.post(ENDPOINTS.HEDGE_MODE, {
        dualSidePosition: hedged ? 'true' : 'false',
      });
      this.applyConfirmedPositionMode(hedged, symbol);
    } catch (err: any) {
      if (isBinancePositionModeAlreadySetError(err)) {
        this.applyConfirmedPositionMode(hedged, symbol);
        return;
      }
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  afterMethodCalledInWorker = <M extends CallableMethodsInWorker>(
    _result: ReturnType<this[M]>,
    method: M,
    ...args: Parameters<this[M]>
  ) => {
    if (method !== 'changePositionMode') return;

    this.applyConfirmedPositionMode(Boolean(args[0]), typeof args[1] === 'string' ? args[1] : undefined);
  };

  private applyConfirmedPositionMode = (hedged: boolean, symbol?: string) => {
    const positionMode = hedged ? PositionMode.Hedged : PositionMode.OneWay;
    const symbols = symbol ? [symbol] : this.store.markets.map((market) => market.symbol);
    const symbolSet = new Set(symbols);
    const fetchedLeverage = { ...this.store.fetchedLeverage };

    for (const symbol of symbols) {
      const previousMetadata = fetchedLeverage[symbol];
      fetchedLeverage[symbol] = {
        ...previousMetadata,
        buyLeverage: previousMetadata?.buyLeverage ?? 0,
        sellLeverage: previousMetadata?.sellLeverage ?? 0,
        positionMode,
      };
    }

    const positions = this.store.positions.map((position) => {
      if (!symbolSet.has(position.symbol) || Number(position.contracts) !== 0) return position;

      return {
        ...position,
        id: createPositionId(position.symbol, position.side, positionMode),
        positionMode,
      };
    });

    this.store.setSetting('isHedged', hedged);
    this.store.update({ fetchedLeverage, positions });
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }

    try {
      await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, {
        symbol,
        marginType: isolated ? 'ISOLATED' : 'CROSSED',
      });
    } catch (e: any) {
      this.emitAndLogError(e?.response?.data?.msg || e?.message);
      throw new Error(e?.response?.data?.msg || e?.message);
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol];
    const position = this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
          symbol,
          leverage,
        });

        this.store.updatePositions(
          this.store.positions.filter((p) => p.symbol === symbol).map((p) => [{ id: p.id }, { leverage }]),
        );
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
  };

  cancelOrders = async (orders: Order[]) => {
    try {
      // Separate regular orders from algo orders
      // For stop/TP/trailing orders, always try algo endpoint first since Tealstreet places them via algo API
      // The order.id IS the algoId for algo orders (set in fetchOrders and WebSocket handler)
      // Regular orders (limit/market) go to the regular endpoint
      const algoOrders = orders.filter((o) => this.isAlgoOrderType(o.type));
      const regularOrders = orders.filter((o) => !this.isAlgoOrderType(o.type));

      // Cancel regular orders (supports batching)
      if (regularOrders.length > 0) {
        const groupedBySymbol = groupBy(regularOrders, 'symbol');
        const requests = Object.entries(groupedBySymbol).map(([symbol, symbolOrders]) => ({
          symbol,
          orders: symbolOrders,
          orderIdList: symbolOrders.map((o) => o.id),
        }));

        await forEachSeries(requests, async (request) => {
          try {
            if (request.orderIdList.length === 1) {
              await this.xhr.delete(ENDPOINTS.ORDER, {
                params: {
                  symbol: request.symbol,
                  orderId: request.orderIdList[0],
                },
              });
            } else {
              const lots = chunk(request.orderIdList, 10);
              await forEachSeries(lots, async (lot) => {
                // Binance expects LIST<LONG> format: [123,456] without string quotes
                // Construct manually to avoid precision loss from parseInt on large IDs
                const orderIdListStr = `[${lot.join(',')}]`;
                await this.xhr.delete(ENDPOINTS.BATCH_ORDERS, {
                  params: {
                    symbol: request.symbol,
                    orderIdList: orderIdListStr,
                  },
                });
              });
            }

            markBinanceOrdersCanceled(this.store, request.orders);
          } catch (err: any) {
            // -2011 "Unknown order sent" means order is already gone - treat as success
            if (err?.code === -2011) {
              markBinanceOrdersCanceled(this.store, request.orders);
              return;
            }
            // Re-throw other errors
            throw err;
          }
        });
      }

      // Cancel algo orders (must be cancelled one at a time)
      if (algoOrders.length > 0) {
        for (const order of algoOrders) {
          try {
            // order.id is the algoId - use camelCase 'algoId' as required by Binance API
            const algoId = order.id;

            await this.xhr.delete(ENDPOINTS.ALGO_ORDER, {
              params: { algoId },
            });

            markBinanceOrdersCanceled(this.store, [order]);
          } catch (err: any) {
            // -2011 "Unknown order sent" means order is already gone - treat as success
            if (err?.code === -2011) {
              markBinanceOrdersCanceled(this.store, [order]);
              continue;
            }
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
            throw err;
          }
        }
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      const symbolOrders = this.store.orders.filter((order) => order.symbol === symbol);
      const regularOrders = symbolOrders.filter((order) => !this.isAlgoOrderType(order.type));
      const algoOrders = symbolOrders.filter((order) => this.isAlgoOrderType(order.type));
      const [regularResult, algoResult] = await Promise.allSettled([
        this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, { params: { symbol } }),
        this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ALGO_ORDERS, { params: { symbol } }),
      ]);

      if (regularResult.status === 'fulfilled') {
        markBinanceOrdersCanceled(this.store, regularOrders);
      }
      if (algoResult.status === 'fulfilled') {
        markBinanceOrdersCanceled(this.store, algoOrders);
      }

      if (regularResult.status === 'rejected') throw regularResult.reason;
      if (algoResult.status === 'rejected') throw algoResult.reason;
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
      timeInForce: order.timeInForce,
      reduceOnly: order.reduceOnly || false,
      close: order.close || false,
    };

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    await this.cancelOrders([order]);
    return await this.placeOrder(newOrder);
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    // Cancel existing closePosition orders if placing a new closePosition order
    // Binance only allows ONE closePosition order per direction per symbol
    await this.cancelExistingClosePositionOrders(opts);

    const { regular, algo } = this.formatCreateOrder(opts);
    const orderIds: string[] = [];

    // Place regular orders (limit/market)
    if (regular.length > 0) {
      const regularIds = await this.placeRegularOrderBatch(regular);
      orderIds.push(...regularIds);
    }

    // Place algo orders (stop/TP/trailing) - must be placed one at a time
    if (algo.length > 0) {
      const algoIds = await this.placeAlgoOrders(algo);
      orderIds.push(...algoIds);
    }

    return orderIds;
  };

  // Cancel existing closePosition orders that would conflict with a new closePosition order
  // Binance only allows one closePosition order per direction per symbol (uses GTE time-in-force internally)
  private cancelExistingClosePositionOrders = async (opts: PlaceOrderOpts) => {
    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;
    if (!isStopOrTP) return;

    // Check if this new order wants to be a closePosition order
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) return;

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);
    const wantsClosePosition = opts.close || !amount;

    if (!wantsClosePosition) return;

    // Use the local store — `cancelOrders` already silently absorbs -2011 ("Unknown order sent"),
    // so a stale row triggers a harmless no-op cancel instead of a REST roundtrip on every placement.
    // The previous fetchOrders here was a major contributor to Binance's 2400 RPM IP rate limit
    // (each call = 2 parallel REST requests, amplified per placement across all accounts on this IP).
    const existingCloseOrders = this.store.orders.filter((o) => {
      return (
        o.symbol === opts.symbol &&
        o.close === true &&
        !o.amount && // Only cancel infinite closePosition orders (no size)
        o.type === opts.type &&
        o.side === opts.side
      );
    });

    if (existingCloseOrders.length > 0) {
      this.log(
        `Canceling ${existingCloseOrders.length} existing closePosition order(s) before placing new one`,
        LogSeverity.Info,
      );
      await this.cancelOrders(existingCloseOrders);
    }
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    const allRegular: any[] = [];
    const allAlgo: any[] = [];

    for (const o of orders) {
      const { regular, algo } = this.formatCreateOrder(o);
      allRegular.push(...regular);
      allAlgo.push(...algo);
    }

    const orderIds: string[] = [];

    // Place regular orders in batches
    if (allRegular.length > 0) {
      const regularIds = await this.placeRegularOrderBatch(allRegular);
      orderIds.push(...regularIds);
    }

    // Place algo orders one at a time
    if (allAlgo.length > 0) {
      const algoIds = await this.placeAlgoOrders(allAlgo);
      orderIds.push(...algoIds);
    }

    return orderIds;
  };

  // Check if order type requires the Algo Order API (as of 2025-12-09)
  private isAlgoOrderType = (type: OrderType): boolean => {
    return type === OrderType.StopLoss || type === OrderType.TakeProfit || type === OrderType.TrailingStopLoss;
  };

  private isPostOnlyRejectionError = (err: any): boolean => {
    const errorCode = err?.response?.data?.code ?? err?.code;
    if (errorCode === -5022) {
      return true;
    }

    const errorMessage = String(err?.response?.data?.msg || err?.message || '').toLowerCase();
    return (
      errorMessage.includes('post only order will be rejected') ||
      errorMessage.includes('could not be executed as maker')
    );
  };

  private formatCreateOrder = (opts: PlaceOrderOpts): { regular: any[]; algo: any[] } => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const regularPayloads: Array<Record<string, any>> = [];
    const algoPayloads: Array<Record<string, any>> = [];

    // Handle trailing stop loss orders (always algo)
    if (opts.type === OrderType.TrailingStopLoss) {
      const trailingPayloads = this.formatCreateTrailingStopLossOrder(opts);
      algoPayloads.push(...trailingPayloads);
      return { regular: regularPayloads, algo: algoPayloads };
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

    const reduceOnly = !this.store.options.isHedged && opts.reduceOnly;
    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    // Determine if this should be a closePosition order
    // closePosition requires an existing position (Binance uses GTE time-in-force internally)
    const wantsClosePosition = isStopOrTP && (opts.close || !amount);

    // Check if position exists for closePosition orders
    const oppositePositionSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const existingPosition = this.store.positions.find(
      (p) => p.symbol === opts.symbol && p.side === oppositePositionSide && Math.abs(p.contracts) > 0,
    );

    // Use closePosition only if requested AND position exists
    const useClosePosition = wantsClosePosition && existingPosition;

    // Build the main order
    if (isStopOrTP) {
      // Algo order format for stop/TP orders
      // closePosition: closes full position when triggered (requires existing position)
      // quantity + reduceOnly: closes specified amount (works without position for bracket orders)
      const algoReq = omitUndefined({
        algoType: 'CONDITIONAL',
        symbol: opts.symbol,
        positionSide: pSide,
        side: inverseObj(ORDER_SIDE)[opts.side],
        type: inverseObj(ORDER_TYPE)[opts.type],
        // If using closePosition, don't send quantity; otherwise send quantity
        quantity: useClosePosition ? undefined : amount ? `${amount}` : undefined,
        triggerPrice: price ? `${price}` : undefined,
        // closePosition and reduceOnly are mutually exclusive
        closePosition: useClosePosition ? 'true' : undefined,
        reduceOnly: useClosePosition ? undefined : reduceOnly ? 'true' : undefined,
        workingType:
          opts.type === OrderType.TakeProfit
            ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
              ? 'MARK_PRICE'
              : 'CONTRACT_PRICE'
            : opts.slTriggerType === OrderStopTriggerType.LastPrice
              ? 'CONTRACT_PRICE'
              : 'MARK_PRICE',
        clientAlgoId: 'x-s3vFFRQp' + uuid(),
      });

      // If closePosition requested but no position exists and no amount provided, error
      if (wantsClosePosition && !existingPosition && !amount) {
        const errMsg = 'Cannot place full stop order without existing position or specified amount';
        this.emitAndLogError(errMsg);
        throw new Error(errMsg);
      }

      algoPayloads.push(algoReq);
    } else {
      // Regular order format for limit/market orders
      const req = omitUndefined({
        symbol: opts.symbol,
        positionSide: pSide,
        side: inverseObj(ORDER_SIDE)[opts.side],
        type: inverseObj(ORDER_TYPE)[opts.type],
        quantity: amount ? `${amount}` : undefined,
        price: price ? `${price}` : undefined,
        timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
        reduceOnly: reduceOnly ? 'true' : undefined,
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

      const clientOrderId = this.resolveCallerClientOrderId(opts);
      this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
      for (const payload of payloads) {
        payload.newClientOrderId = clientOrderId ?? 'x-s3vFFRQp' + uuid();
      }

      regularPayloads.push(...payloads);
    }

    // Handle attached stop loss (always algo order)
    // For bracket orders, position doesn't exist yet so we use quantity + reduceOnly
    // instead of closePosition (which requires existing position)
    if (opts.stopLoss) {
      const slPSide = this.getOrderPositionSide({
        ...opts,
        side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        type: OrderType.StopLoss,
        reduceOnly: true,
      });
      algoPayloads.push(
        omitUndefined({
          algoType: 'CONDITIONAL',
          symbol: opts.symbol,
          positionSide: slPSide,
          side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
          type: inverseObj(ORDER_TYPE)[OrderType.StopLoss],
          triggerPrice: `${adjust(opts.stopLoss, pPrice)}`,
          quantity: amount ? `${amount}` : undefined,
          // reduceOnly cannot be sent in Hedge Mode
          reduceOnly: this.store.options.isHedged ? undefined : 'true',
          workingType: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'CONTRACT_PRICE' : 'MARK_PRICE',
          clientAlgoId: 'x-s3vFFRQp' + uuid(),
        }),
      );
    }

    // Handle attached take profit (always algo order)
    // For bracket orders, position doesn't exist yet so we use quantity + reduceOnly
    if (opts.takeProfit) {
      const tpPSide = this.getOrderPositionSide({
        ...opts,
        side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        type: OrderType.TakeProfit,
        reduceOnly: true,
      });
      algoPayloads.push(
        omitUndefined({
          algoType: 'CONDITIONAL',
          symbol: opts.symbol,
          positionSide: tpPSide,
          side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
          type: inverseObj(ORDER_TYPE)[OrderType.TakeProfit],
          triggerPrice: `${adjust(opts.takeProfit, pPrice)}`,
          quantity: amount ? `${amount}` : undefined,
          // reduceOnly cannot be sent in Hedge Mode
          reduceOnly: this.store.options.isHedged ? undefined : 'true',
          workingType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'CONTRACT_PRICE',
          clientAlgoId: 'x-s3vFFRQp' + uuid(),
        }),
      );
    }

    return { regular: regularPayloads, algo: algoPayloads };
  };

  private formatCreateTrailingStopLossOrder = (opts: PlaceOrderOpts) => {
    // Use O(1) hashmap lookup instead of O(n) find
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

    // Use Algo Order format for trailing stop (as of 2025-12-09)
    const payload = {
      algoType: 'CONDITIONAL',
      symbol: opts.symbol,
      positionSide: this.getOrderPositionSide(opts),
      side: inverseObj(ORDER_SIDE)[opts.side],
      type: inverseObj(ORDER_TYPE)[OrderType.TrailingStopLoss],
      quantity: `${position.contracts}`,
      callbackRate: `${distancePercentage}`,
      priceProtect: 'TRUE',
      clientAlgoId: 'x-s3vFFRQp' + uuid(),
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

  // Place regular orders (limit/market) - supports batching
  private placeRegularOrderBatch = async (payloads: any[]) => {
    const lots = chunk(payloads, 5);
    const orderIds = [] as string[];

    for (const lot of lots) {
      if (lot.length === 1) {
        try {
          const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, lot[0]);
          // Use exchange's orderId (not clientOrderId) to match WebSocket events
          orderIds.push(data.orderId?.toString());
        } catch (err: any) {
          if (!this.isPostOnlyRejectionError(err)) {
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
          }
          throw err;
        }
      }

      if (lot.length > 1) {
        try {
          const { data } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, {
            batchOrders: JSON.stringify(lot),
          });

          data?.forEach?.((o: any) => {
            if (o.code) {
              if (!this.isPostOnlyRejectionError(o)) {
                this.emitAndLogError(o.msg);
              }
            } else {
              // Use exchange's orderId (not clientOrderId) to match WebSocket events
              orderIds.push(o.orderId?.toString());
            }
          });
        } catch (err: any) {
          if (!this.isPostOnlyRejectionError(err)) {
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
          }
          throw err;
        }
      }
    }

    return orderIds;
  };

  // Place algo orders (stop/TP/trailing) - must be placed one at a time (no batch support)
  private placeAlgoOrders = async (payloads: any[]) => {
    const orderIds = [] as string[];

    for (const payload of payloads) {
      try {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.ALGO_ORDER, payload);
        // Use exchange's algoId (not clientAlgoId) to match WebSocket events
        orderIds.push(data.algoId?.toString());
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    return orderIds;
  };
}
