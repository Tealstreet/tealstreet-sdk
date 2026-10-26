import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

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
  CoinBalance,
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
  PublicTrade,
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
import { BaseExchange, CallableMethodsInWorker } from '../../../exchanges/base';
import { createAPI, createPublicAPI } from './binanceinverse.api';
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
} from '../../../exchanges/binance/binanceinverse.types';
import { BinanceInversePrivateWebsocket } from './binanceinverse.ws-private';
import { BinanceInversePublicWebsocket } from './binanceinverse.ws-public';

type BinanceInverseResponseRecord = Record<string, any>;

const describeBinanceInverseResponseShape = (payload: unknown): string => {
  if (Array.isArray(payload)) return `array(${payload.length})`;
  if (payload === null) return 'null';
  if (typeof payload === 'object') {
    const keys = Object.keys(payload as Record<string, unknown>).slice(0, 6);
    return keys.length ? `object(${keys.join(',')})` : 'object';
  }
  return typeof payload;
};

const getBinanceInverseResponseMessage = (payload: unknown): string => {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return '';
  const record = payload as Record<string, unknown>;
  const message = record.msg || record.message || record.error;
  const code = record.code;
  return [code !== undefined ? `code=${String(code)}` : '', message ? String(message) : ''].filter(Boolean).join(' ');
};

const requireBinanceInverseArrayResponse = <T>(endpoint: string, payload: unknown): T[] => {
  if (Array.isArray(payload)) return payload as T[];
  const shape = describeBinanceInverseResponseShape(payload);
  const message = getBinanceInverseResponseMessage(payload);
  throw new Error(
    `Binance COIN-M ${endpoint} response malformed: expected array, received ${shape}${
      message ? ` (${message})` : ''
    }`,
  );
};

const PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS = 3_000;
const ACCOUNT_RECONCILIATION_INTERVAL_MS = 25_000;
const DEGRADED_ACCOUNT_RECONCILIATION_INTERVAL_MS = 3_000;
const POSITION_RISK_RECONCILIATION_INTERVAL_MS = 75_000;
const ORDER_RETRY_INTERVAL_MS = 10_000;
const REQUEST_WEIGHT_LIMIT_PER_MINUTE = 2_400;
const REQUEST_WEIGHT_BACKOFF_THRESHOLD = REQUEST_WEIGHT_LIMIT_PER_MINUTE * 0.8;
const REQUEST_WEIGHT_BACKOFF_MS = 60_000;

const isBinancePositionModeAlreadySetError = (err: any) => {
  const code = err?.response?.data?.code;
  return code === -4059 || code === '-4059';
};

export class BinanceInverseExchange extends BaseExchange {
  xhr: Axios;
  _primaryXhr: Axios;
  _secondaryXhr: Axios;
  unlimitedXHR: Axios;
  publicXhr: Axios;

  publicWebsocket: BinanceInversePublicWebsocket;
  privateWebsocket: BinanceInversePrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;
  ordersSnapshotStale = false;

  private hasLoggedBalanceAggregation = false;
  private positionStateVersion = 0;
  private lastAccountReconciliationAt = 0;
  private lastPositionRiskReconciliationAt = 0;
  private backgroundReconciliationPausedUntil = 0;
  private lastOrdersRetryAt = 0;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    const [xhr, primaryXhr, secondaryXhr] = createApiWithSecondaryKey(opts, this, createAPI, {
      rateLimit: (client) => rateLimit(client, { maxRPS: 3 }),
    });

    this.xhr = xhr;
    this._primaryXhr = primaryXhr;
    this._secondaryXhr = secondaryXhr;
    this.publicXhr = createPublicAPI(opts, this);
    this.unlimitedXHR = createAPI(opts, this);
    this.installRequestWeightGuards([primaryXhr, secondaryXhr, this.publicXhr, this.unlimitedXHR]);

    this.publicWebsocket = new BinanceInversePublicWebsocket(this);
    this.privateWebsocket = new BinanceInversePrivateWebsocket(this);
  }

  markPositionStateUpdatedByWebsocket = () => {
    this.positionStateVersion += 1;
  };

  getPositionStateVersion = () => this.positionStateVersion;

  isBackgroundReconciliationPaused = () => Date.now() < this.backgroundReconciliationPausedUntil;

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

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  validateAccount = async () => {
    let balance: Balance | undefined;
    try {
      const { data } = await this._primaryXhr.get(ENDPOINTS.ACCOUNT);
      const total = this.usdValueForAssetBalances(data?.assets || [], 'walletBalance');
      if (Number.isFinite(total)) {
        const free = this.usdValueForAssetBalances(data?.assets || [], 'availableBalance');
        const upnl = this.usdValueForAssetBalances(data?.assets || [], 'unrealizedProfit');
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

  private loadAccountMaxLeverage = () =>
    this.loadAccountMarketOverlay(async () => {
      const { data } = await this.xhr.get<unknown>(ENDPOINTS.LEVERAGE_BRACKET);
      const brackets = requireBinanceInverseArrayResponse<Record<string, any>>(ENDPOINTS.LEVERAGE_BRACKET, data);
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
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;
      this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;
      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Binance COIN-M markets`);
      this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
      this.publicWebsocket.connectAndSubscribe();
    }

    if (!this.isPublicDataOnly) await this._startPrivate();
    else await this._startPublicDataOnly();
  };

  _startPrivate = async () => {
    this.privateWebsocket.connectAndSubscribe();
    this.store.setSetting('isHedged', await this.fetchPositionMode());
    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Binance COIN-M`);
    const orders = await this.fetchOrders();
    if (this.isDisposed || this.ordersSnapshotStale) return;
    this.log(`Loaded Binance COIN-M orders`);
    this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
  };

  _startPublicDataOnly = async () => {
    this.log(`Ready to watch public data on Binance COIN-M`);
    this.store.update({
      orders: [],
      balance: { used: 0, free: 0, total: 0, upnl: 0 },
      positions: [],
      loaded: { ...this.store.loaded, orders: true, balance: true, positions: true },
    });
  };

  private retryOrdersSnapshot = async () => {
    if (Date.now() - this.lastOrdersRetryAt < ORDER_RETRY_INTERVAL_MS) return;
    this.lastOrdersRetryAt = Date.now();
    const orders = await this.fetchOrders();
    if (this.isDisposed || this.ordersSnapshotStale) return;
    this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
  };

  tick = async (_count = 0) => {
    if (this.isDisposed) return;
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
      (this.lastAccountReconciliationAt === 0 || now - this.lastAccountReconciliationAt >= accountReconciliationInterval);

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
      }

      if (shouldReconcileAccount && !this.isBackgroundReconciliationPaused()) {
        const positionStateVersion = this.getPositionStateVersion();
        const { balance, positions } = await this.fetchPositionsAndBalance();
        if (this.isDisposed) return;
        this.store.update({
          balance,
          ...(positionStateVersion === this.getPositionStateVersion() && { positions }),
          loaded: { ...this.store.loaded, balance: true, positions: true },
        });
        this.lastAccountReconciliationAt = Date.now();
      }
    } catch (err: any) {
      this.emitAndLogError(err?.message);
    }

    if (!this.isDisposed) loop(() => this.tick(), PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS);
  };

  fetchMarkets = async () => {
    try {
      const { data: marketData } = await this.publicXhr.get<{ symbols?: unknown }>(ENDPOINTS.MARKETS);
      const symbols = requireBinanceInverseArrayResponse<BinanceInverseResponseRecord>(
        `${ENDPOINTS.MARKETS}.symbols`,
        marketData?.symbols,
      );
      const { data: leverageBracketData } = this.isPublicDataOnly
        ? { data: [] }
        : await this.xhr.get<unknown>(ENDPOINTS.LEVERAGE_BRACKET);
      const leverageBrackets = requireBinanceInverseArrayResponse<BinanceInverseResponseRecord>(
        ENDPOINTS.LEVERAGE_BRACKET,
        leverageBracketData,
      );

      return symbols
        .filter((m) => v(m, 'contractType') === 'PERPETUAL' && v(m, 'contractStatus') === 'TRADING')
        .map((m): Market => {
          const filters = Array.isArray(m.filters) ? m.filters : [];
          const priceFilter = filters.find((f: any) => v(f, 'filterType') === 'PRICE_FILTER');
          const lotFilter = filters.find((f: any) => v(f, 'filterType') === 'LOT_SIZE');
          const marketLotFilter = filters.find((f: any) => v(f, 'filterType') === 'MARKET_LOT_SIZE');
          const brackets = leverageBrackets.find((b) => b.symbol === m.symbol)?.brackets;
          const baseAsset = v(m, 'baseAsset');
          const quoteAsset = v(m, 'quoteAsset') || 'USD';
          const marginAsset = v(m, 'marginAsset') || baseAsset;
          const contractSize = parseFloat(v(m, 'contractSize') || '1');
          return {
            id: `${baseAsset}/${quoteAsset}:${marginAsset}`,
            symbol: m.symbol,
            base: baseAsset,
            quote: quoteAsset,
            active: m.contractStatus === 'TRADING',
            precision: {
              amount: parseFloat(v(lotFilter, 'stepSize')) || 1,
              price: parseFloat(v(priceFilter, 'tickSize')),
            },
            limits: {
              amount: {
                min: Math.max(parseFloat(v(lotFilter, 'minQty')) || 1, parseFloat(v(marketLotFilter, 'minQty')) || 1),
                max: Math.min(
                  parseFloat(v(lotFilter, 'maxQty')) || Infinity,
                  parseFloat(v(marketLotFilter, 'maxQty')) || Infinity,
                ),
              },
              leverage: { min: 1, max: v(brackets?.[0], 'initialLeverage') || 1 },
            },
            contractSize,
            isInverse: true,
            info: m,
          };
        });
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    try {
      const [{ data: dailyData }, { data: bookData }, { data: priceData }] = await Promise.all([
        this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_24H),
        this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_BOOK),
        this.publicXhr.get<unknown>(ENDPOINTS.TICKERS_PRICE),
      ]);
      const dailys = requireBinanceInverseArrayResponse<BinanceInverseResponseRecord>(ENDPOINTS.TICKERS_24H, dailyData);
      const books = requireBinanceInverseArrayResponse<BinanceInverseResponseRecord>(ENDPOINTS.TICKERS_BOOK, bookData);
      const prices = requireBinanceInverseArrayResponse<BinanceInverseResponseRecord>(ENDPOINTS.TICKERS_PRICE, priceData);
      const marketsBySymbol = this.store.marketsBySymbol;
      return books.reduce((acc: Ticker[], book) => {
        const market = marketsBySymbol[book.symbol];
        const daily = dailys.find((d) => d.symbol === book.symbol);
        const price = prices.find((p) => p.symbol === book.symbol);
        if (!market || !daily || !price) return acc;
        acc.push({
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
          volume: parseFloat(v(daily, 'volume')),
          quoteVolume: parseFloat(v(daily, 'baseVolume') || v(daily, 'quoteVolume') || '0'),
          openInterest: 0,
        });
        return acc;
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  private nativeContractsToBase = (symbol: string, nativeContracts: number, fallbackPrice = 0) => {
    const market = this.store.marketsBySymbol[symbol];
    const ticker = this.store.tickersBySymbol[symbol];
    const referencePrice = ticker?.mark || ticker?.last || fallbackPrice;
    const contractSize = market?.contractSize || 1;
    return referencePrice > 0 ? Math.abs(nativeContracts) * contractSize / referencePrice : 0;
  };

  private nativeContractsToNotional = (symbol: string, nativeContracts: number) => {
    const contractSize = this.store.marketsBySymbol[symbol]?.contractSize || 1;
    return Math.abs(nativeContracts) * contractSize;
  };

  private mapPosition = (p: Record<string, any>, mode: PositionMode): Position => {
    const nativeContracts = parseFloat(p.positionAmt || '0');
    const pSide = p.positionSide;
    const side =
      (pSide in POSITION_SIDE && POSITION_SIDE[pSide]) || (nativeContracts >= 0 ? PositionSide.Long : PositionSide.Short);
    const positionMode = p.positionSide === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
    const entryPrice = parseFloat(p.entryPrice || '0');
    const markPrice = parseFloat(p.markPrice || '0');
    const notional = this.nativeContractsToNotional(p.symbol, nativeContracts);
    const leverage = parseFloat(p.leverage || '0');
    const isolatedMargin = parseFloat(v(p, 'isolatedMargin') || '0');
    const marginSize =
      isolatedMargin > 0
        ? isolatedMargin
        : leverage > 0 && (markPrice || entryPrice) > 0
          ? this.nativeContractsToBase(p.symbol, nativeContracts, markPrice || entryPrice) / leverage
          : undefined;
    return {
      id: createPositionId(p.symbol, side, positionMode || mode),
      symbol: p.symbol,
      side,
      entryPrice,
      notional,
      leverage,
      unrealizedPnl: parseFloat(p.unRealizedProfit || '0'),
      contracts: this.nativeContractsToBase(p.symbol, nativeContracts, markPrice || entryPrice),
      liquidationPrice: parseFloat(p.liquidationPrice || '0'),
      marginSize,
      marginType: String(p.marginType || '').toLowerCase() === 'isolated' ? MarginType.Isolated : MarginType.Cross,
      positionMode: positionMode || mode,
      info: { ...p, contractsNative: Math.abs(nativeContracts) },
    };
  };

  fetchPositions = async () => {
    try {
      const { data } = await this.xhr.get<GET_POSITION_RISK_RESPONSE>(ENDPOINTS.POSITION_RISK);
      const rows = requireBinanceInverseArrayResponse<Record<string, any>>(ENDPOINTS.POSITION_RISK, data);
      const mode = this.store.options.isHedged ? PositionMode.Hedged : PositionMode.OneWay;
      const formattedPositions = rows.map((p) => this.mapPosition(p, mode));
      const returnedIds = new Set(formattedPositions.map((p) => p.id));
      const fakePositions = this.store.markets.flatMap((market): Position[] =>
        [PositionSide.Long, PositionSide.Short].map((side) => {
          const id = createPositionId(market.symbol, side, mode);
          return {
            id,
            symbol: market.symbol,
            side,
            entryPrice: 0,
            notional: 0,
            leverage: 0,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            marginSize: 0,
            marginType: MarginType.Cross,
            positionMode: mode,
            info: { synthetic: true },
          };
        }),
      );
      return uniqBy([...formattedPositions, ...fakePositions.filter((p) => !returnedIds.has(p.id))], 'id');
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.positions;
    }
  };

  private getAssetUsdPrice = (asset: string) => {
    if (asset === 'USD' || asset === 'USDT' || asset === 'USDC') return 1;
    const direct =
      this.store.tickersBySymbol[`${asset}USD_PERP`] ||
      this.store.tickersBySymbol[`${asset}USD`] ||
      this.store.tickersBySymbol[`${asset}USDT`];
    return direct?.mark || direct?.last || 0;
  };

  private usdValueForAssetBalances = (assets: Array<Record<string, any>>, field: string) =>
    assets.reduce((sum, asset) => {
      const amount = parseFloat(asset[field] || '0');
      if (!amount) return sum;
      const price = this.getAssetUsdPrice(asset.asset);
      return sum + (price > 0 ? amount * price : amount);
    }, 0);

  fetchPositionsAndBalance = async () => {
    try {
      const { data } = await this.xhr.get<Record<string, any>>(ENDPOINTS.ACCOUNT);
      const assets = Array.isArray(data?.assets) ? data.assets : [];
      let total = 0;
      let free = 0;
      let upnl = 0;
      let used = 0;
      const currencies: Record<string, Balance> = {};
      const coins: Record<string, CoinBalance> = {};

      for (const asset of assets) {
        const price = this.getAssetUsdPrice(asset.asset);
        const wallet = parseFloat(asset.walletBalance || '0');
        const available = parseFloat(asset.availableBalance || '0');
        const unrealized = parseFloat(asset.unrealizedProfit || '0');
        const initial = parseFloat(asset.initialMargin || '0');
        if (wallet === 0 && available === 0 && unrealized === 0 && initial === 0) continue;
        const usdMultiplier = price > 0 ? price : 1;
        total += wallet * usdMultiplier;
        free += available * usdMultiplier;
        used += initial * usdMultiplier;
        upnl += unrealized * usdMultiplier;
        currencies[asset.asset] = { total: wallet, free: available, used: initial, upnl: unrealized };
        coins[asset.asset] = { base: asset.asset, amount: wallet, notional: wallet * usdMultiplier };
      }

      if (!this.hasLoggedBalanceAggregation && Object.keys(coins).length > 0) {
        this.hasLoggedBalanceAggregation = true;
        this.log(
          `COIN-M balance aggregation: ${Object.keys(coins).join(', ')}; Total: $${total.toFixed(2)}, Free: $${free.toFixed(
            2,
          )}, Used: $${used.toFixed(2)}, UPnL: $${upnl.toFixed(2)}`,
          LogSeverity.Info,
        );
      }

      const balance = { total, free, used, upnl, currencies, coins, info: { ...data, assets: undefined, coinMargined: true } };
      return { balance, positions: this.store.positions };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { balance: this.store.balance, positions: this.store.positions };
    }
  };

  fetchOrders = async () => {
    try {
      const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.OPEN_ORDERS);
      const ordersData = Array.isArray(data) ? data : [];
      const orders = ordersData.map((o): Order => {
        const type = ORDER_TYPE[v(o, 'type')];
        return {
          id: v(o, 'orderId')?.toString(),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type,
          side: ORDER_SIDE[o.side],
          price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')) || 0,
          amount: parseFloat(v(o, 'origQty')),
          reduceOnly: v(o, 'reduceOnly') || false,
          close: v(o, 'closePosition') || false,
          filled: parseFloat(v(o, 'executedQty')),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: subtract(v(o, 'origQty'), v(o, 'executedQty')),
          info: o,
          ...(type === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
          ...(type === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
        };
      });
      this.ordersSnapshotStale = false;
      return orders;
    } catch (error: any) {
      this.log(`Binance COIN-M fetchOrders error: ${error?.message || error}`, LogSeverity.Error);
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
    const { data } = await this.publicXhr.get<any[][]>(ENDPOINTS.KLINE, {
      params: { symbol: opts.symbol, interval, startTime: start.valueOf(), endTime: end.valueOf(), ...(opts.limit ? { limit } : {}) },
    });
    if (!Array.isArray(data)) return [];
    return data.map(([time, open, high, low, close, volume]): Candle => ({
      timestamp: time / 1000,
      open: parseFloat(open),
      high: parseFloat(high),
      low: parseFloat(low),
      close: parseFloat(close),
      volume: parseFloat(volume),
    }));
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    if (this.externalPublicDataProvider) return this.externalPublicDataProvider.subscribeCandle(opts, callback);
    let disposed = false;
    const unsubscribe = this.publicWebsocket.listenOHLCV(opts, callback);
    void this.fetchOHLCV({ ...opts, limit: 1 })
      .then((candles) => {
        if (disposed || this.isDisposed) return;
        const latest = candles[candles.length - 1];
        if (latest) callback(latest);
      })
      .catch((err: any) => this.log(`Unable to seed Binance COIN-M live candle: ${err?.message || err}`, LogSeverity.Debug));
    return () => {
      disposed = true;
      unsubscribe();
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    if (this.externalPublicDataProvider) return this.externalPublicDataProvider.subscribeOrderbook(symbol, callback);
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchPublicTrades = async (symbol: string, limit?: number): Promise<PublicTrade[]> => {
    const { data } = await this.publicXhr.get(ENDPOINTS.TRADES, { params: { symbol, limit: Math.min(limit || 1000, 1000) } });
    if (!Array.isArray(data)) return [];
    return data.map((trade: any): PublicTrade => ({
      id: trade.id.toString(),
      symbol,
      price: parseFloat(trade.price),
      qty: parseFloat(trade.qty),
      quoteQty: parseFloat(trade.baseQty || trade.quoteQty || '0'),
      timestamp: trade.time,
      side: trade.isBuyerMaker ? OrderSide.Sell : OrderSide.Buy,
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
      await this.xhr.post(ENDPOINTS.HEDGE_MODE, { dualSidePosition: hedged ? 'true' : 'false' });
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
    for (const currentSymbol of symbols) {
      const previousMetadata = fetchedLeverage[currentSymbol];
      fetchedLeverage[currentSymbol] = {
        ...previousMetadata,
        buyLeverage: previousMetadata?.buyLeverage ?? 0,
        sellLeverage: previousMetadata?.sellLeverage ?? 0,
        positionMode,
      };
    }
    const positions = this.store.positions.map((position) =>
      symbolSet.has(position.symbol) && Number(position.contracts) === 0
        ? { ...position, id: createPositionId(position.symbol, position.side, positionMode), positionMode }
        : position,
    );
    this.store.setSetting('isHedged', hedged);
    this.store.update({ fetchedLeverage, positions });
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) throw new Error('Symbol is required');
    try {
      await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, { symbol, marginType: isolated ? 'ISOLATED' : 'CROSSED' });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw new Error(err?.response?.data?.msg || err?.message);
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
        await this.xhr.post(ENDPOINTS.SET_LEVERAGE, { symbol, leverage });
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
      const groupedBySymbol = groupBy(orders, 'symbol');
      await forEachSeries(Object.entries(groupedBySymbol), async ([symbol, symbolOrders]) => {
        const lots = chunk(symbolOrders, 10);
        await forEachSeries(lots, async (lot) => {
          if (lot.length === 1) {
            await this.xhr.delete(ENDPOINTS.ORDER, { params: { symbol, orderId: lot[0].id } });
          } else {
            const orderIdList = `[${lot.map((o) => o.id).join(',')}]`;
            await this.xhr.delete(ENDPOINTS.BATCH_ORDERS, { params: { symbol, orderIdList } });
          }
          this.store.updateOrdersStatus(lot.map((o) => ({ id: o.id })), OrderStatus.Canceled);
        });
      });
    } catch (err: any) {
      if (err?.code === -2011) return;
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      await this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, { params: { symbol } });
      this.store.updateOrdersStatus(this.store.orders.filter((order) => order.symbol === symbol), OrderStatus.Canceled);
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
    const payloads = this.formatCreateOrder(opts);
    return await this.placeOrderPayloads(payloads);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    const payloads = orders.flatMap((order) => this.formatCreateOrder(order));
    return await this.placeOrderPayloads(payloads);
  };

  formatCreateOrder = (opts: PlaceOrderOpts): Array<Record<string, any>> => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    const pSide = this.getOrderPositionSide(opts);
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;
    const reduceOnly = !this.store.options.isHedged && opts.reduceOnly;
    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];
    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;
    const wantsClosePosition = isStopOrTP && (opts.close || !amount);
    const oppositePositionSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const existingPosition = this.store.positions.find(
      (p) => p.symbol === opts.symbol && p.side === oppositePositionSide && Math.abs(p.contracts) > 0,
    );
    if (wantsClosePosition && !existingPosition && !amount) {
      const message = 'Cannot place full stop order without existing position or specified amount';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const baseReq = omitUndefined({
      symbol: opts.symbol,
      positionSide: pSide,
      side: inverseObj(ORDER_SIDE)[opts.side],
      type: inverseObj(ORDER_TYPE)[opts.type],
      quantity: wantsClosePosition && existingPosition ? undefined : amount ? `${amount}` : undefined,
      price: opts.type === OrderType.Limit ? `${price}` : undefined,
      stopPrice: isStopOrTP && price ? `${price}` : undefined,
      closePosition: wantsClosePosition && existingPosition ? 'true' : undefined,
      reduceOnly: wantsClosePosition && existingPosition ? undefined : reduceOnly ? 'true' : undefined,
      workingType:
        opts.type === OrderType.TakeProfit
          ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
            ? 'MARK_PRICE'
            : 'CONTRACT_PRICE'
          : opts.slTriggerType === OrderStopTriggerType.LastPrice
            ? 'CONTRACT_PRICE'
            : 'MARK_PRICE',
      timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
      callbackRate: opts.type === OrderType.TrailingStopLoss && opts.price ? `${this.trailingCallbackRate(opts)}` : undefined,
      activationPrice: opts.type === OrderType.TrailingStopLoss && opts.price ? `${price}` : undefined,
      newClientOrderId: 'x-s3vFFRQp' + uuid(),
    });

    const payloads: Array<Record<string, any>> = this.splitPayloadByMaxSize(baseReq, amount, market);

    if (opts.stopLoss) {
      payloads.push(this.createAttachedStopPayload(opts, OrderType.StopLoss, opts.stopLoss));
    }
    if (opts.takeProfit) {
      payloads.push(this.createAttachedStopPayload(opts, OrderType.TakeProfit, opts.takeProfit));
    }
    return payloads;
  };

  private trailingCallbackRate = (opts: PlaceOrderOpts) => {
    const ticker = this.store.tickersBySymbol[opts.symbol];
    if (!ticker || !opts.price) return undefined;
    const priceDistance = Math.max(ticker.last, opts.price) - Math.min(ticker.last, opts.price);
    return Math.max(0.1, Math.min(10, Math.round(((priceDistance * 100) / ticker.last) * 10) / 10));
  };

  private createAttachedStopPayload = (opts: PlaceOrderOpts, type: OrderType.StopLoss | OrderType.TakeProfit, trigger: number) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    const pPrice = market.precision.price;
    const amount = adjust(opts.amount, market.precision.amount);
    const side = opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;
    const positionSide = this.getOrderPositionSide({ ...opts, side, type, reduceOnly: true });
    return omitUndefined({
      symbol: opts.symbol,
      positionSide,
      side: inverseObj(ORDER_SIDE)[side],
      type: inverseObj(ORDER_TYPE)[type],
      quantity: amount ? `${amount}` : undefined,
      stopPrice: `${adjust(trigger, pPrice)}`,
      reduceOnly: this.store.options.isHedged ? undefined : 'true',
      workingType:
        type === OrderType.StopLoss
          ? opts.slTriggerType === OrderStopTriggerType.LastPrice
            ? 'CONTRACT_PRICE'
            : 'MARK_PRICE'
          : opts.tpTriggerType === OrderStopTriggerType.MarkPrice
            ? 'MARK_PRICE'
            : 'CONTRACT_PRICE',
      newClientOrderId: 'x-s3vFFRQp' + uuid(),
    });
  };

  private splitPayloadByMaxSize = (req: Record<string, any>, amount: number, market: Market) => {
    if (!amount || req.closePosition) return [req];
    const maxSize = market.limits.amount.max;
    if (!Number.isFinite(maxSize) || amount <= maxSize) return [req];
    const lots = Math.ceil(amount / maxSize);
    const rest = adjust(amount % maxSize, market.precision.amount);
    const lotSize = adjust((amount - rest) / lots, market.precision.amount);
    const payloads = times(lots, () => ({ ...req, quantity: `${lotSize}`, newClientOrderId: 'x-s3vFFRQp' + uuid() }));
    if (rest) payloads.push({ ...req, quantity: `${rest}`, newClientOrderId: 'x-s3vFFRQp' + uuid() });
    return payloads;
  };

  private placeOrderPayloads = async (payloads: Array<Record<string, any>>) => {
    const orderIds: string[] = [];
    for (const lot of chunk(payloads, 5)) {
      if (lot.length === 1) {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, lot[0]);
        orderIds.push(data.orderId?.toString());
      } else {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, { batchOrders: JSON.stringify(lot) });
        data?.forEach?.((o: any) => {
          if (o.code) this.emitAndLogError(o.msg);
          else orderIds.push(o.orderId?.toString());
        });
      }
    }
    return orderIds.filter(Boolean);
  };

  private getOrderPositionSide = (opts: PlaceOrderOpts) => {
    let positionSide = 'BOTH';
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
}
