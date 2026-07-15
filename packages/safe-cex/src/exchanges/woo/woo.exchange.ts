import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import flatten from 'lodash/flatten';
import sumBy from 'lodash/sumBy';
import times from 'lodash/times';
import { forEachSeries, mapSeries } from 'p-iteration';

import { createApiWithSecondaryKey } from '../../createApiWithSecondaryKey';
import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalTrade,
  HistoricalTradeOptions,
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
  timeframeToSeconds,
  UpdateOrderOpts,
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { getCandleStartTime } from '../../utils/getCandleStartTime';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust, subtract } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange, CallableMethodsInWorker } from '../base';
import { createAPI } from './woo.api';
import {
  ACCOUNT_RESPONSE,
  ENDPOINTS,
  FETCH_ORDER_RESPONSE,
  GET_FUTURES_LEVERAGE_RESPONSE,
  GET_KLINE_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  PLACE_ALGO_ORDER_RESPONSE,
  PLACE_ORDER_RESPONSE,
} from './woo.types';
import { normalizeSymbol, reverseSymbol } from './woo.utils';
import { WooPrivateWebscoket } from './woo.ws-private';
import { WooPublicWebsocket } from './woo.ws-public';

export class WOOXExchange extends BaseExchange {
  private static readonly MAX_FILL_POSITION_REFRESH_ATTEMPTS = 4;
  private static readonly FILL_POSITION_REFRESH_BASE_DELAY_MS = 150;

  xhr: Axios;
  _primaryXhr: Axios;
  _secondaryXhr: Axios;
  _proxyEnforcedXhr: Axios;

  publicWebsocket: WooPublicWebsocket;
  privateWebsocket: WooPrivateWebscoket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // Woo store leverage per account, not per position
  // as workaround we store the account leverage here when
  // we call the ACCOUNT endpoint, and copy this value into positions
  // private accountLeverage = 1;
  private _accountPositionMode: PositionMode | null = null;
  private _fillPositionRefreshTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private _fillPositionRefreshInFlight = new Set<string>();
  private _fillExpectedPositionSignatureBySymbol = new Map<string, string>();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    const [xhr, primaryXhr, secondaryXhr] = createApiWithSecondaryKey(opts, this, createAPI, {
      rateLimit: (xhr) =>
        rateLimit(
          rateLimit(xhr, {
            perMilliseconds: 1000,
            maxRequests: 4,
          }),
          {
            maxRequests: 20,
            perMilliseconds: 6_000,
          },
        ),
    });

    this.xhr = xhr;
    this._primaryXhr = primaryXhr;
    this._secondaryXhr = secondaryXhr;

    this._proxyEnforcedXhr = createAPI(
      {
        ...this.options,
        tealstreetProxy: !this.options.tealstreetTunnel,
      },
      this,
    );

    this.publicWebsocket = new WooPublicWebsocket(this);
    this.privateWebsocket = new WooPrivateWebscoket(this);

    this.on('fill', this.handleFillForPositionSync);
  }

  dispose = () => {
    this.isDisposed = true;
    this.off('fill', this.handleFillForPositionSync);
    this._fillPositionRefreshTimers.forEach((timer) => clearTimeout(timer));
    this._fillPositionRefreshTimers.clear();
    this._fillPositionRefreshInFlight.clear();
    this._fillExpectedPositionSignatureBySymbol.clear();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  async getPositionMode(): Promise<PositionMode> {
    if (!this._accountPositionMode) {
      const {
        data: { data },
      } = await this.xhr.get<{ data: ACCOUNT_RESPONSE }>(ENDPOINTS.ACCOUNT);
      this._accountPositionMode = data.positionMode === 'ONE_WAY' ? PositionMode.OneWay : PositionMode.Hedged;
    }
    return this._accountPositionMode;
  }

  afterMethodCalledInWorker = <M extends CallableMethodsInWorker>(
    result: ReturnType<this[M]>,
    method: M,
    ...args: Parameters<this[M]>
  ) => {
    if (method === 'changePositionMode') {
      this._accountPositionMode = args[0] ? PositionMode.Hedged : PositionMode.OneWay;
    }
  };

  private normalizeTimestampSeconds = (raw: unknown): number | undefined => {
    if (raw === undefined || raw === null || raw === '') {
      return undefined;
    }

    const parsed = Number(raw);
    if (!Number.isFinite(parsed) || parsed < 0) {
      return undefined;
    }

    return parsed >= 1_000_000_000_000 ? Math.floor(parsed / 1000) : Math.floor(parsed);
  };

  private normalizeTimestampMilliseconds = (raw: unknown): number | undefined => {
    if (raw === undefined || raw === null || raw === '') {
      return undefined;
    }

    const parsed = Number(raw);
    if (!Number.isFinite(parsed) || parsed < 0) {
      return undefined;
    }

    return parsed >= 1_000_000_000_000 ? Math.floor(parsed) : Math.floor(parsed * 1000);
  };

  private normalizeTransactionSymbol = (raw: unknown): string | undefined => {
    if (typeof raw !== 'string' || raw.length === 0) {
      return undefined;
    }

    return normalizeSymbol(raw.replace(/\//g, '_'));
  };

  private normalizeRequestedSymbol = (raw?: string): string | undefined => {
    if (!raw) return undefined;
    const stripped = raw.toUpperCase().split(':').pop() || raw.toUpperCase();
    const withoutPerpSuffix = stripped
      .replace(/\.P$/, '')
      .replace(/_PERP$/, '')
      .replace(/-PERP$/, '')
      .replace(/PERP$/, '');
    return this.normalizeTransactionSymbol(withoutPerpSuffix);
  };

  private getTransactionHistorySymbolCandidates = (symbol: string): string[] => {
    const uppercaseSymbol = symbol.toUpperCase();
    const base = uppercaseSymbol.endsWith('USDT') ? uppercaseSymbol.slice(0, -4) : uppercaseSymbol;

    const candidates = [reverseSymbol(uppercaseSymbol), `${base}_USDT_PERP`, `${base}-USDT-PERP`, uppercaseSymbol];

    return Array.from(new Set(candidates.filter(Boolean)));
  };

  private parseTransactionHistoryRow = (row: Record<string, any>) => {
    const normalizedSymbol = this.normalizeTransactionSymbol(v(row, 'symbol'));
    if (!normalizedSymbol) {
      return null;
    }

    const rawSide = `${v(row, 'side') || ''}`.toUpperCase();
    const side =
      ORDER_SIDE[rawSide] || (rawSide === 'BUY' ? OrderSide.Buy : rawSide === 'SELL' ? OrderSide.Sell : undefined);
    if (side !== OrderSide.Buy && side !== OrderSide.Sell) {
      return null;
    }

    const price = Number(
      v(row, 'executedPrice') ??
        v(row, 'price') ??
        v(row, 'averageExecutedPrice') ??
        v(row, 'average_executed_price') ??
        v(row, 'lastPrice'),
    );
    const qty = Number(v(row, 'executedQuantity') ?? v(row, 'quantity') ?? v(row, 'executed') ?? v(row, 'lastQty'));
    const timestampMilliseconds = this.normalizeTimestampMilliseconds(
      v(row, 'executedTimestamp') ||
        v(row, 'executed_time') ||
        v(row, 'tradeTime') ||
        v(row, 'timestamp') ||
        v(row, 'created_time') ||
        v(row, 'createdTime') ||
        v(row, 'updated_time') ||
        v(row, 'updatedTime'),
    );
    if (
      !Number.isFinite(price) ||
      price <= 0 ||
      !Number.isFinite(qty) ||
      qty <= 0 ||
      timestampMilliseconds === undefined
    ) {
      return null;
    }

    const rawOrderId = v(row, 'orderId') ?? v(row, 'order_id') ?? v(row, 'id');
    const orderId = rawOrderId === undefined || rawOrderId === null || rawOrderId === '' ? undefined : `${rawOrderId}`;

    const rawCommission = Number(v(row, 'fee') ?? v(row, 'total_fee'));
    const commission = Number.isFinite(rawCommission) ? rawCommission : undefined;
    const commissionAsset = v(row, 'feeAsset') ?? v(row, 'fee_asset');

    const rawRealizedPnl = Number(v(row, 'realizedPnl') ?? v(row, 'realized_pnl'));
    const realizedPnl = Number.isFinite(rawRealizedPnl) ? rawRealizedPnl : undefined;

    const rawPositionSide = `${v(row, 'positionSide') ?? v(row, 'position_side') ?? 'BOTH'}`.toUpperCase();
    const positionSide =
      rawPositionSide === 'LONG'
        ? PositionSide.Long
        : rawPositionSide === 'SHORT'
          ? PositionSide.Short
          : ('both' as const);

    return {
      symbol: normalizedSymbol,
      side,
      price,
      qty,
      timestampMilliseconds,
      timestampSeconds: Math.floor(timestampMilliseconds / 1000),
      orderId,
      commission,
      commissionAsset: commissionAsset || undefined,
      realizedPnl,
      positionSide,
    };
  };

  private parseTransactionRowsFromResponse = (data: any): Record<string, any>[] => {
    if (Array.isArray(data?.data?.rows)) return data.data.rows;
    if (Array.isArray(data?.rows)) return data.rows;
    if (Array.isArray(data?.data)) return data.data;
    if (Array.isArray(data)) return data;
    return [];
  };

  private fetchLegacyTransactionHistoryRows = async (opts: {
    symbol?: string;
    fromMilliseconds?: number;
    toMilliseconds?: number;
    limit: number;
  }): Promise<Record<string, any>[]> => {
    const endpoints = [ENDPOINTS.CLIENT_TRADES, ENDPOINTS.HIST_TRADES];
    const symbolCandidates = opts.symbol ? this.getTransactionHistorySymbolCandidates(opts.symbol) : [];
    const paramsFor = (symbolCandidate?: string) => {
      const params: Record<string, any> = { limit: opts.limit };
      if (symbolCandidate) params.symbol = symbolCandidate;
      if (opts.fromMilliseconds !== undefined) params.startTime = opts.fromMilliseconds;
      if (opts.toMilliseconds !== undefined) params.endTime = opts.toMilliseconds;
      return params;
    };

    for (const endpoint of endpoints) {
      for (const symbolCandidate of symbolCandidates) {
        try {
          const { data } = await this.xhr.get<any>(endpoint, { params: paramsFor(symbolCandidate) });
          const rows = this.parseTransactionRowsFromResponse(data);
          if (rows.length > 0) return rows;
        } catch {
          // Legacy endpoints are best-effort fallback only.
        }
      }
      try {
        const { data } = await this.xhr.get<any>(endpoint, { params: paramsFor() });
        const rows = this.parseTransactionRowsFromResponse(data);
        if (rows.length > 0) return rows;
      } catch {
        // Legacy endpoints are best-effort fallback only.
      }
    }

    return [];
  };

  private fetchTransactionHistoryRows = async (opts: {
    symbol?: string;
    fromMilliseconds?: number;
    toMilliseconds?: number;
    limit: number;
  }): Promise<Record<string, any>[]> => {
    const buildParams = (symbolCandidate?: string) => {
      const params: Record<string, any> = { limit: opts.limit };
      if (symbolCandidate) {
        params.symbol = symbolCandidate;
      }
      if (opts.fromMilliseconds !== undefined) {
        params.startTime = opts.fromMilliseconds;
      }
      if (opts.toMilliseconds !== undefined) {
        params.endTime = opts.toMilliseconds;
      }
      return params;
    };

    const requestRows = async (params: Record<string, any>) => {
      const { data } = await this.xhr.get<any>(ENDPOINTS.TRANSACTION_HISTORY, { params });
      return this.parseTransactionRowsFromResponse(data);
    };

    if (!opts.symbol) {
      return requestRows(buildParams());
    }

    let lastError: any;
    for (const symbolCandidate of this.getTransactionHistorySymbolCandidates(opts.symbol)) {
      try {
        const rows = await requestRows(buildParams(symbolCandidate));
        if (rows.length > 0) {
          return rows;
        }
      } catch (err) {
        lastError = err;
      }
    }

    try {
      const rows = await requestRows(buildParams());
      if (rows.length > 0) {
        return rows;
      }
    } catch (err) {
      lastError = err;
    }

    const legacyRows = await this.fetchLegacyTransactionHistoryRows(opts);
    if (legacyRows.length > 0) {
      return legacyRows;
    }

    if (lastError) {
      throw lastError;
    }

    return [];
  };

  private getPositionSignatureForSymbol = (symbol: string) => {
    return this.store.positions
      .filter((position) => position.symbol === symbol)
      .sort((a, b) => a.id.localeCompare(b.id))
      .map((position) => `${position.id}:${position.side}:${position.contracts}:${position.entryPrice}`)
      .join('|');
  };

  private scheduleFillPositionRefresh = (symbol: string, attempt: number) => {
    if (this.isDisposed) return;

    const existingTimer = this._fillPositionRefreshTimers.get(symbol);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }

    const delayMs =
      attempt === 0 ? 0 : WOOXExchange.FILL_POSITION_REFRESH_BASE_DELAY_MS * Math.pow(2, Math.max(attempt - 1, 0));

    const timer = setTimeout(() => {
      this._fillPositionRefreshTimers.delete(symbol);
      this.refreshPositionForSymbolFromFill(symbol, attempt).catch((err) => {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
      });
    }, delayMs);

    this._fillPositionRefreshTimers.set(symbol, timer);
  };

  private refreshPositionForSymbolFromFill = async (symbol: string, attempt: number) => {
    if (this.isDisposed) return;
    if (this._fillPositionRefreshInFlight.has(symbol)) return;

    this._fillPositionRefreshInFlight.add(symbol);
    let shouldRetry = false;

    try {
      const beforeSignature = this.getPositionSignatureForSymbol(symbol);
      const expectedSignature = this._fillExpectedPositionSignatureBySymbol.get(symbol);
      const positions = await this.fetchPositions([symbol]);
      if (this.isDisposed) return;

      this.store.addOrUpdatePositions(positions);
      const afterSignature = this.getPositionSignatureForSymbol(symbol);
      shouldRetry = expectedSignature ? afterSignature !== expectedSignature : beforeSignature === afterSignature;

      if (!shouldRetry) {
        this._fillExpectedPositionSignatureBySymbol.delete(symbol);
      }
    } catch (err: any) {
      shouldRetry = true;
      this.emitAndLogError(err?.response?.data?.message || err?.message);
    } finally {
      this._fillPositionRefreshInFlight.delete(symbol);
    }

    if (shouldRetry && attempt + 1 < WOOXExchange.MAX_FILL_POSITION_REFRESH_ATTEMPTS) {
      this.scheduleFillPositionRefresh(symbol, attempt + 1);
      return;
    }

    this._fillExpectedPositionSignatureBySymbol.delete(symbol);
  };

  private applyOptimisticOneWayPositionFromFill = (orderFill: {
    symbol?: string;
    side?: OrderSide;
    price?: number;
    amount?: number;
  }) => {
    if (this._accountPositionMode !== PositionMode.OneWay) {
      return false;
    }

    const symbol = orderFill?.symbol;
    const side = orderFill?.side;
    const fillPrice = Number(orderFill?.price);
    const fillQty = Number(orderFill?.amount);

    if (!symbol) return false;
    if (side !== OrderSide.Buy && side !== OrderSide.Sell) return false;
    if (!Number.isFinite(fillPrice) || fillPrice <= 0) return false;
    if (!Number.isFinite(fillQty) || fillQty <= 0) return false;

    const currentOpenPosition = this.store.positions.find(
      (position) => position.symbol === symbol && position.contracts > 0,
    );
    const signedCurrentContracts = !currentOpenPosition
      ? 0
      : currentOpenPosition.side === PositionSide.Long
        ? currentOpenPosition.contracts
        : -currentOpenPosition.contracts;
    const signedFillContracts = side === OrderSide.Buy ? fillQty : -fillQty;
    let nextSignedContracts = signedCurrentContracts + signedFillContracts;

    if (Math.abs(nextSignedContracts) < 1e-10) {
      nextSignedContracts = 0;
    }

    const nextContracts = Math.abs(nextSignedContracts);
    const nextSide = nextSignedContracts >= 0 ? PositionSide.Long : PositionSide.Short;

    if (currentOpenPosition && (nextContracts === 0 || currentOpenPosition.side !== nextSide)) {
      this.store.updatePosition(
        { id: currentOpenPosition.id },
        {
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
        },
      );
    }

    if (nextContracts === 0) {
      return true;
    }

    const ticker = this.store.tickersBySymbol[symbol];
    const mark = Number(ticker?.mark);
    const markOrFillPrice = Number.isFinite(mark) && mark > 0 ? mark : fillPrice;

    const previousContracts = Math.abs(signedCurrentContracts);
    const sameDirection =
      Math.sign(signedCurrentContracts) === Math.sign(nextSignedContracts) && Math.sign(signedCurrentContracts) !== 0;

    let nextEntryPrice = fillPrice;
    if (currentOpenPosition && sameDirection && nextContracts <= previousContracts) {
      nextEntryPrice = currentOpenPosition.entryPrice;
    } else if (currentOpenPosition && sameDirection && previousContracts > 0) {
      nextEntryPrice =
        (currentOpenPosition.entryPrice * previousContracts + fillPrice * Math.abs(signedFillContracts)) /
        nextContracts;
    }

    const positionMode = currentOpenPosition?.positionMode || PositionMode.OneWay;
    const leverage = currentOpenPosition?.leverage || 1;
    const marginType = currentOpenPosition?.marginType || MarginType.Cross;

    const optimisticPosition: Position = {
      id: createPositionId(symbol, nextSide, positionMode),
      symbol,
      side: nextSide,
      entryPrice: nextEntryPrice,
      notional: nextContracts * markOrFillPrice,
      leverage,
      unrealizedPnl: nextContracts === 0 ? 0 : currentOpenPosition?.unrealizedPnl || 0,
      contracts: nextContracts,
      liquidationPrice: nextContracts === 0 ? 0 : currentOpenPosition?.liquidationPrice || 0,
      positionMode,
      marginType,
    };

    this.store.addOrUpdatePosition(optimisticPosition);
    return true;
  };

  private handleFillForPositionSync = (orderFill: {
    symbol?: string;
    side?: OrderSide;
    price?: number;
    amount?: number;
  }) => {
    const symbol = orderFill?.symbol;
    if (!symbol) return;

    const didApplyOptimisticPosition = this.applyOptimisticOneWayPositionFromFill(orderFill);
    const hasOpenPositionAfterOptimisticUpdate = this.store.positions.some(
      (position) => position.symbol === symbol && position.contracts > 0,
    );
    if (didApplyOptimisticPosition && hasOpenPositionAfterOptimisticUpdate) {
      this._fillExpectedPositionSignatureBySymbol.set(symbol, this.getPositionSignatureForSymbol(symbol));
    } else {
      this._fillExpectedPositionSignatureBySymbol.delete(symbol);
    }
    this.scheduleFillPositionRefresh(symbol, 0);
  };

  getAccount = async () => {
    const {
      data: { data },
    } = await this.xhr.get<{ data: ACCOUNT_RESPONSE }>(ENDPOINTS.ACCOUNT);

    return {
      userId: data.applicationId,
      affiliateId: data.referrerID || null,
    };
  };

  validateAccount = async () => {
    try {
      await this._primaryXhr.get(ENDPOINTS.ACCOUNT);
      if (this.options.secondary_key) {
        await this._secondaryXhr.get(ENDPOINTS.ACCOUNT);
      }
      // Key validated; balance is best-effort for the new-account gate.
      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return {
        error:
          err?.response?.data?.message ||
          err?.response?.data?.msg ||
          err?.message ||
          'Invalid API key or secret',
      };
    }
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

      if (this.isDisposed) {
        return;
      }

      const tickers = this.parseTickers(rawTickers);
      this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
    }

    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, orders: true, positions: true, balance: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    await Promise.all([
      this.fetchBalance().then((balance) => {
        if (this.isDisposed) return;
        this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
        return balance;
      }),
      this.fetchPositions().then((positions) => {
        if (this.isDisposed) return;
        this.store.addOrUpdatePositionsFull(positions);
        this.store.update({ loaded: { ...this.store.loaded, positions: true } });
        return positions;
      }),
      this.fetchOrders().then((orders) => {
        if (this.isDisposed) return;
        this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        return orders;
      }),
    ]);

    if (this.isDisposed) return;
    await this.tick();

    this.log(`Ready to trade on WOO X`);
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        // after fetch account in case positionMode or leverage updated
        if (count % 5 === 0) {
          const { positions, balance } = await this.fetchPositionsAndBalance();
          if (this.isDisposed) return;
          this.store.addOrUpdatePositionsFull(positions);
          this.store.update({ balance, loaded: { ...this.store.loaded, balance: true, positions: true } });
        } else {
          const positions = await this.fetchPositions();
          if (this.isDisposed) return;
          this.store.addOrUpdatePositionsFull(positions);
        }

        if ((count + 5) % 10 === 0) {
          const orders = await this.fetchOrders();
          if (this.isDisposed) return;
          this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        }
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
      const {
        data: { data },
      } = await this.xhr.get<{ data: ACCOUNT_RESPONSE }>(ENDPOINTS.ACCOUNT);

      if (data.accountMode !== 'FUTURES') {
        throw new Error(`WOO X account is in ${data.accountMode} mode, only FUTURES mode is supported`);
      }

      // same with positionMode
      this._accountPositionMode = data.positionMode === 'ONE_WAY' ? PositionMode.OneWay : PositionMode.Hedged;

      const free = Math.round(data.freeCollateral * 100) / 100;
      const total = Math.round(data.totalAccountValue * 100) / 100;
      const used = Math.round((data.totalCollateral - data.freeCollateral) * 100) / 100;

      // woo doesnt provides unrealized pnl in the account endpoint
      // we are computing this from the positions polling
      return { free, total, used, upnl: this.store.balance.upnl };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.balance;
    }
  };

  fetchMarkets = async () => {
    try {
      const {
        data: { rows },
      } = await this.xhr.get<{ rows: Array<Record<string, any>> }>(ENDPOINTS.MARKETS);

      const markets: Market[] = rows
        .filter((r) => r.symbol.startsWith('PERP_'))
        .map((r) => {
          const [, baseAsset, quoteAsset] = r.symbol.split('_');
          const imr = v(r, 'base_imr') || 0.01;
          const maxLeverage = Math.floor(1 / imr);

          return {
            id: `${baseAsset}/${quoteAsset}:USDT`,
            symbol: `${baseAsset}${quoteAsset}`,
            base: baseAsset,
            quote: quoteAsset,
            active: true,
            precision: {
              amount: v(r, 'base_tick'),
              price: v(r, 'quote_tick'),
            },
            limits: {
              amount: {
                min: v(r, 'base_min'),
                max: v(r, 'base_max'),
              },
              leverage: {
                min: 1,
                max: maxLeverage,
              },
            },
          };
        });

      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    const rows = await this.fetchTickersRaw();
    return this.parseTickers(rows);
  };

  fetchTickersRaw = async () => {
    try {
      const {
        data: { rows },
      } = await this.xhr.get<{ rows: Array<Record<string, any>> }>(ENDPOINTS.TICKERS);

      return rows;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return null;
    }
  };

  parseTickers = (rows: any) => {
    try {
      if (!rows) return this.store.tickers;

      return rows.reduce((acc: Ticker[], row: any) => {
        const symbol = normalizeSymbol(row.symbol);
        const market = this.store.marketsBySymbol[symbol];

        if (!market) return acc;

        const open = v(row, '24h_open');
        const close = v(row, '24h_close');

        const percentage = Math.round(((close - open) / open) * 100 * 100) / 100;

        const ticker: Ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: close,
          ask: close,
          last: close,
          index: v(row, 'index_price'),
          mark: v(row, 'mark_price'),
          percentage,
          fundingRate: v(row, 'last_funding_rate'),
          fundingTime: v(row, 'next_funding_time'),
          volume: v(row, '24h_volume'),
          quoteVolume: v(row, '24h_amount'),
          openInterest: v(row, 'open_interest'),
        };

        return [...acc, ticker];
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  fetchPositionsAndBalance = async () => {
    // call before positions to update leverage
    const balance = await this.fetchBalance();
    const positions = await this.fetchPositions();

    // woo doesnt provides unrealized pnl in the account endpoint
    // we are computing this from the positions polling
    (balance as Writable<Balance>).upnl =
      positions.length > 0 ? Math.round(sumBy(positions, 'unrealizedPnl') * 100) / 100 : 0;

    // total balance from API already takes in account uPNL
    (balance as Writable<Balance>).total = balance.total - balance.upnl;
    return { positions, balance };
  };

  fetchPositionMetadata = async (
    symbols: string[],
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
    try {
      const positionMode = await this.getPositionMode();
      return flatten(
        await mapSeries(
          symbols,
          async (symbol): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
            if (this.isDisposed) return [];

            const { data } = await this.xhr.get<GET_FUTURES_LEVERAGE_RESPONSE>(ENDPOINTS.FUTURES_LEVERAGE, {
              params: {
                symbol: reverseSymbol(symbol),
                // woo is fucked and always uses oneway leverage
                // position_mode: positionMode === PositionMode.OneWay ? 'ONE_WAY' : 'HEDGE_MODE',
                position_mode: 'ONE_WAY',
                margin_mode: 'CROSS',
              },
            });

            if (this.isDisposed) return [];

            if (positionMode === PositionMode.OneWay) {
              const detail = data.data.details[0];
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage: parseInt(detail.leverage),
                  positionMode,
                  marginType: MarginType.Cross,
                },
              ];
            } else {
              // const longDetail = data.data.details.find((d) => d.position_side === 'LONG');
              // const shortDetail = data.data.details.find((d) => d.position_side === 'SHORT');
              const detail = data.data.details[0];
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  // leverage: parseInt(longDetail?.leverage || '1'),
                  leverage: parseInt(detail.leverage),
                  positionMode,
                  marginType: MarginType.Cross,
                },
                {
                  symbol,
                  side: PositionSide.Short,
                  // leverage: parseInt(shortDetail?.leverage || '1'),
                  leverage: parseInt(detail.leverage),
                  positionMode,
                  marginType: MarginType.Cross,
                },
              ];
            }
          },
        ),
      );
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchPositions = async (symbols?: string[]) => {
    const openPositions = await this.fetchOpenPositions();
    const positionMode = await this.getPositionMode();
    const targetSymbols = symbols || [
      ...new Set([
        ...openPositions.map((p) => p.symbol),
        ...this.store.orders.filter((o) => o.status === OrderStatus.Open).map((o) => o.symbol),
      ]),
    ];

    if (!targetSymbols.length) return openPositions;

    const openPositionsForSymbols = openPositions.filter((p) => targetSymbols.includes(p.symbol));
    const symbolsNeedingMetadata = targetSymbols.filter((symbol) => {
      const symbolPositions = openPositionsForSymbols.filter((p) => p.symbol === symbol);
      if (!symbolPositions.length) return true;
      if (positionMode !== PositionMode.Hedged) return false;
      const hasLong = symbolPositions.some((p) => p.side === PositionSide.Long);
      const hasShort = symbolPositions.some((p) => p.side === PositionSide.Short);
      return !hasLong || !hasShort;
    });
    const positionMetadatas = await this.fetchPositionMetadata(symbolsNeedingMetadata);
    const openPositionIds = new Set(
      openPositionsForSymbols.map((p) => createPositionId(p.symbol, p.side, p.positionMode || positionMode)),
    );
    const fakePositions = positionMetadatas
      .filter(
        (metadata) => !openPositionIds.has(createPositionId(metadata.symbol, metadata.side, metadata.positionMode)),
      )
      .map((metadata) => {
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
    if (symbols) {
      return [...openPositionsForSymbols, ...fakePositions];
    }

    return [...openPositions, ...fakePositions];
  };

  // will not update leverage if called by itself (need to call fetchBalance first)
  fetchOpenPositions = async () => {
    try {
      const { data } = await this.xhr.get<{
        data: { positions: Array<Record<string, any>> };
      }>(ENDPOINTS.POSITIONS);

      if (!(data as any)?.success) {
        throw new Error((data as any).message || `Failed to fetch positions`);
      }

      const positionMode = await this.getPositionMode();
      const positions = data?.data?.positions || [];
      return positions.reduce<Position[]>((acc, p) => {
        const symbol = normalizeSymbol(p.symbol);
        const ticker = this.store.tickersBySymbol[symbol];

        if (!ticker) return acc;
        if (!p.holding) return acc;

        const entryPrice = v(p, 'averageOpenPrice');
        const holdings = v(p, 'holding');
        const contracts = Math.abs(holdings);
        const side = holdings > 0 ? PositionSide.Long : PositionSide.Short;

        const priceDiff = Math.max(entryPrice, ticker.mark) - Math.min(entryPrice, ticker.mark);

        const isLoss =
          (side === PositionSide.Long && ticker.mark < entryPrice) ||
          (side === PositionSide.Short && ticker.mark > entryPrice);

        const absUPNL = priceDiff * contracts;
        const upnl = Math.round((isLoss ? -absUPNL : absUPNL) * 100) / 100;

        const position: Position = {
          id: createPositionId(symbol, side, positionMode),
          symbol,
          side,
          entryPrice,
          notional: contracts * ticker.mark,
          leverage: p.leverage,
          unrealizedPnl: upnl,
          contracts,
          liquidationPrice: v(p, 'estLiqPrice'),
          positionMode: positionMode,
          marginType: p.marginMode === 'CROSS' ? MarginType.Cross : MarginType.Isolated,
          info: p,
        };

        if (position.marginType === MarginType.Isolated) {
          this.emitAndLogError('Detected isolated margin position. Only cross margin is supported');
        }

        return [...acc, position];
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.positions;
    }
  };

  fetchOrders = async () => {
    const limitOrders = await this.fetchLimitOrders();
    const algoOrders = await this.fetchAlgoOrders();
    return [...limitOrders, ...algoOrders];
  };

  fetchExecutions = async (opts: ExecutionsOptions = {}): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 1000 } = opts;
    const normalizedRequestedSymbol = this.normalizeRequestedSymbol(symbol);
    const normalizedOrderId = orderId ? String(orderId) : undefined;
    const fromSeconds = this.normalizeTimestampSeconds(from);
    const toSeconds = this.normalizeTimestampSeconds(to);
    const maxLimit = Math.min(Math.max(limit, 1), 1000);
    const fromMilliseconds = this.normalizeTimestampMilliseconds(from);
    const toMilliseconds = this.normalizeTimestampMilliseconds(to);

    try {
      const rows = await this.fetchTransactionHistoryRows({
        symbol: normalizedRequestedSymbol,
        fromMilliseconds,
        toMilliseconds,
        limit: maxLimit,
      });

      const executions = rows
        .map((row: Record<string, any>) => this.parseTransactionHistoryRow(row))
        .filter((entry) => {
          if (!entry) {
            return false;
          }
          if (normalizedRequestedSymbol && entry.symbol !== normalizedRequestedSymbol) {
            return false;
          }
          if (normalizedOrderId && entry.orderId !== normalizedOrderId) {
            return false;
          }
          if (fromSeconds !== undefined && entry.timestampSeconds < fromSeconds) {
            return false;
          }
          if (toSeconds !== undefined && entry.timestampSeconds > toSeconds) {
            return false;
          }
          return true;
        })
        .map((entry): Execution => {
          return omitUndefined({
            timestamp: entry!.timestampSeconds,
            symbol: entry!.symbol,
            orderId: entry!.orderId,
            price: entry!.price,
            qty: entry!.qty,
            side: entry!.side,
            commission: entry!.commission,
            commissionAsset: entry!.commissionAsset,
          });
        })
        .sort((a, b) => b.timestamp - a.timestamp)
        .slice(0, maxLimit);

      if (normalizedRequestedSymbol && executions.length === 0) {
        const historicalTrades = await this.fetchHistoricalTrades({
          symbol: normalizedRequestedSymbol,
          orderId: normalizedOrderId,
          from,
          to,
          limit: Math.max(maxLimit, 1000),
        });

        if (historicalTrades.length > 0) {
          return historicalTrades
            .map((trade): Execution => {
              return omitUndefined({
                timestamp: Math.floor(trade.time / 1000),
                symbol: trade.symbol,
                orderId: trade.orderId,
                price: trade.price,
                qty: trade.qty,
                side: trade.side,
                commission: trade.commission,
                commissionAsset: trade.commissionAsset,
              });
            })
            .sort((a, b) => b.timestamp - a.timestamp)
            .slice(0, maxLimit);
        }
      }

      return executions;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchHistoricalTrades = async (opts: HistoricalTradeOptions): Promise<HistoricalTrade[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 1000 } = opts;
    const normalizedRequestedSymbol = this.normalizeRequestedSymbol(symbol);
    if (!normalizedRequestedSymbol) {
      return [];
    }
    const normalizedOrderId = orderId ? String(orderId) : undefined;
    const maxLimit = Math.min(Math.max(limit, 1), 1000);
    const fromMilliseconds = this.normalizeTimestampMilliseconds(from);
    const toMilliseconds = this.normalizeTimestampMilliseconds(to);

    try {
      const rows = await this.fetchTransactionHistoryRows({
        symbol: normalizedRequestedSymbol,
        fromMilliseconds,
        toMilliseconds,
        limit: maxLimit,
      });

      return rows
        .map((row: Record<string, any>) => this.parseTransactionHistoryRow(row))
        .filter((entry) => {
          if (!entry) {
            return false;
          }
          if (entry.symbol !== normalizedRequestedSymbol) {
            return false;
          }
          if (normalizedOrderId && entry.orderId !== normalizedOrderId) {
            return false;
          }
          if (fromMilliseconds !== undefined && entry.timestampMilliseconds < fromMilliseconds) {
            return false;
          }
          if (toMilliseconds !== undefined && entry.timestampMilliseconds > toMilliseconds) {
            return false;
          }
          return true;
        })
        .map((entry): HistoricalTrade => {
          return omitUndefined({
            symbol: entry!.symbol,
            orderId: entry!.orderId || `${entry!.symbol}-${entry!.timestampMilliseconds}`,
            price: entry!.price,
            qty: entry!.qty,
            realizedPnl: entry!.realizedPnl,
            positionSide: entry!.positionSide,
            side: entry!.side,
            commission: entry!.commission,
            commissionAsset: entry!.commissionAsset,
            time: entry!.timestampMilliseconds,
          });
        })
        .sort((a, b) => b.time - a.time)
        .slice(0, maxLimit);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const currentCandleStart = getCandleStartTime(opts.interval, Date.now() / 1000);
    const intervalInSeconds = timeframeToSeconds[opts.interval] || Infinity;
    const toInSeconds = (opts.to || 0) / 1000;
    const distanceFromCurrent = Math.abs(currentCandleStart - toInSeconds) / intervalInSeconds;
    let rows;

    // if we can get newest candle within 1000 limit + 500 fetchable candles, we should use KLINE bc that has newest in progress candle
    if (distanceFromCurrent < 100) {
      const { data } = await this.xhr.get<GET_KLINE_RESPONSE['data']>(ENDPOINTS.KLINE, {
        params: {
          symbol: reverseSymbol(opts.symbol),
          type: opts.interval,
          limit: Math.min(opts.limit || 500, 1000),
        },
      });
      rows = data.rows;
    } else {
      const { data } = await this._proxyEnforcedXhr.get<GET_KLINE_RESPONSE>(ENDPOINTS.KLINE_HISTORY, {
        params: {
          symbol: reverseSymbol(opts.symbol),
          type: opts.interval,
          start_time: opts.from,
          end_time: opts.to,
          size: Math.min(opts.limit || 500, 1000),
        },
      });
      rows = data.data.rows;
    }

    const candles: Candle[] = rows.map((r) => {
      return {
        timestamp: r.start_timestamp / 1000,
        open: r.open,
        high: r.high,
        low: r.low,
        close: r.close,
        volume: r.volume,
      };
    });

    return candles.reverse();
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    if (order.type === OrderType.TrailingStopLoss) {
      await this.cancelOrders([order]);

      return this.placeTrailingStopLossOrder({
        symbol: order.symbol,
        side: order.side,
        type: OrderType.TrailingStopLoss,
        price: 'price' in update ? update.price : order.price,
        amount: 'amount' in update ? update.amount : order.amount,
      });
    }

    if (this.isAlgoOrder(order.type)) {
      return this.updateAlgoOrder({ order, update });
    }

    const payload: Record<string, any> = { orderId: order.id };
    if ('price' in update) payload.price = `${update.price}`;
    if ('amount' in update) payload.quantity = `${update.amount}`;

    try {
      await this.xhr.put(`${ENDPOINTS.UPDATE_ORDER}/${order.id}`, payload);
      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const payload: Record<string, any> = {};

    if ('price' in update) {
      if (order.amount) {
        payload.triggerPrice = `${update.price}`;
      } else {
        payload.childOrders = [{ algoOrderId: order.id, triggerPrice: `${update.price}` }];
      }
    }

    try {
      await this.xhr.put(`${ENDPOINTS.ALGO_ORDER}/${order.parentId}`, payload);
      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  fetchOrdersRecursively = async (endpoint: string, page: number = 1): Promise<FETCH_ORDER_RESPONSE['rows']> => {
    let { data } = await this.xhr.get<FETCH_ORDER_RESPONSE>(endpoint, {
      params: { status: 'INCOMPLETE', page },
    });

    if (this.isDisposed) return [];

    if (!data.success) {
      this.emitAndLogError((data as any).message || (data as any).meta?.message || `Failed to fetch orders`);
      return [];
    }

    // algo orders nest data...
    data = (data as any).data || data;

    if (data.rows.length === 0) return [];

    if (data.meta.total > data.meta.records_per_page * page) {
      return [...data.rows, ...(await this.fetchOrdersRecursively(endpoint, page + 1))];
    }

    return data.rows;
  };

  fetchLimitOrders = async () => {
    try {
      const rows = await this.fetchOrdersRecursively(ENDPOINTS.ORDERS);

      return rows.reduce<Order[]>((acc, o) => {
        const order = this.mapLimitOrder(o);
        return order !== null ? [...acc, order] : acc;
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  mapLimitOrder = (o: Record<string, any>) => {
    const orderId = v(o, 'order_id') || v(o, 'orderId') || v(o, 'id');
    const rawSymbol = v(o, 'symbol');
    if (!orderId || !rawSymbol) return null;

    const symbol = normalizeSymbol(rawSymbol);
    const market = this.store.marketsBySymbol[symbol];
    if (!market) return null;
    const quantity = o.quantity || o.order_quantity;
    const executed = o.executed || 0;

    const timeInForce = o.type === 'POST_ONLY' ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel;

    const status = o.status === 'CANCELLED' ? OrderStatus.Canceled : OrderStatus.Open;
    const order: Order = {
      id: `${orderId}`,
      status,
      symbol,
      type: ORDER_TYPE[o.type || o.order_type],
      side: ORDER_SIDE[o.side],
      price: o.price || o.order_price,
      amount: quantity,
      reduceOnly: v(o, 'reduce_only'),
      timeInForce,
      filled: executed,
      remaining: subtract(quantity, executed),
      lastUpdate: virtualClock.getCurrentTimeMs(),
      info: o,
    };

    return order;
  };

  fetchAlgoOrders = async () => {
    try {
      const rows = await this.fetchOrdersRecursively(ENDPOINTS.ALGO_ORDERS);

      return rows.reduce<Order[]>((acc, o) => {
        const childOrders = this.mapAlgoOrder(o);
        return [...acc, ...childOrders];
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  mapAlgoOrder = (o: Record<string, any>) => {
    const symbol = normalizeSymbol(o.symbol);
    const market = this.store.marketsBySymbol[symbol];

    if (!market) return [];

    const arr = o.childOrders ? o.childOrders : [o];
    return arr
      .filter((co: Record<string, any>) => v(co, 'triggerPrice') || v(co, 'algoType') === 'TRAILING_STOP')
      .map((co: Record<string, any>) => {
        const filled = v(co, 'totalExecutedQuantity');
        const price = v(co, 'triggerPrice') || subtract(v(co, 'extremePrice'), v(co, 'callbackValue'));
        const orderType = ORDER_TYPE[v(co, 'algoType')];

        return {
          id: `${v(co, 'algoOrderId')}`,
          parentId: `${v(o, 'rootAlgoOrderId') || v(co, 'rootAlgoOrderId')}`,
          status: OrderStatus.Open,
          symbol,
          type: orderType,
          ...(() => {
            const rawTriggerPriceType = v(co, 'triggerPriceType');
            const mappedTriggerType = ORDER_TRIGGER_TYPE[rawTriggerPriceType];
            return orderType === OrderType.TrailingStopLoss || orderType === OrderType.StopLoss
              ? { slTriggerType: mappedTriggerType }
              : { tpTriggerType: mappedTriggerType };
          })(),
          side: ORDER_SIDE[co.side],
          price,
          amount: co.quantity,
          reduceOnly: v(co, 'reduceOnly'),
          filled,
          remaining: subtract(co.quantity, filled),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          info: o,
        };
      });
  };

  cancelOrders = async (orders: Order[]) => {
    // filter out orders with same parent id
    const seenParentIds = new Set<string>();
    orders = orders.filter((o) => {
      if (o.parentId && seenParentIds.has(o.parentId)) return false;
      seenParentIds.add(o.parentId!);
      return true;
    });

    try {
      await forEachSeries(orders, async (order) => {
        const isAlgo = this.isAlgoOrder(order.type);

        if (isAlgo) {
          await this.xhr.delete(`${ENDPOINTS.ALGO_ORDER}/${order.parentId}`);
        } else {
          await this.xhr.delete(ENDPOINTS.CANCEL_ORDER, {
            data: omitUndefined({
              order_id: parseInt(order.id, 10),
              symbol: reverseSymbol(order.symbol),
            }),
          });
        }
      });
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  cancelAlgoOrder = async (order: Order, recreateSibling = true) => {
    try {
      const sibling = this.store.orders.find((o) => o.parentId === order.parentId && o.id !== order.id);

      const id = order.parentId || order.id;
      await this.xhr.delete(`${ENDPOINTS.ALGO_ORDER}/${id}`);

      if (sibling && recreateSibling) {
        const priceKey = sibling.type === OrderType.StopLoss ? 'stopLoss' : 'takeProfit';
        const triggerKey = sibling.type === OrderType.StopLoss ? 'slTriggerType' : 'tpTriggerType';

        await this.placePositionalAlgoOrder({
          symbol: sibling.symbol,
          side: sibling.side,
          [priceKey]: sibling.price,
          [triggerKey]: sibling[triggerKey],
        });
      }
    } catch (err: any) {
      if (err?.response?.data?.message?.includes('already canceled')) {
        this.store.updateOrderStatus({ id: order.id }, OrderStatus.Canceled);
      } else {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
        throw err;
      }
    }
  };

  getPositionSideForOrder = async (opts: Pick<PlaceOrderOpts, 'side' | 'reduceOnly'>) => {
    const positionMode = await this.getPositionMode();
    const reduceOnly = opts.reduceOnly;
    let positionSide = undefined;
    if (positionMode === PositionMode.Hedged) {
      if (reduceOnly) {
        if (opts.side === OrderSide.Buy) {
          positionSide = 'SHORT';
        } else {
          positionSide = 'LONG';
        }
      } else {
        if (opts.side === OrderSide.Buy) {
          positionSide = 'LONG';
        } else {
          positionSide = 'SHORT';
        }
      }
    }
    return positionSide;
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    if (this.isAlgoOrder(opts.type)) {
      return this.placeAlgoOrder(opts);
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market not found: ${opts.symbol}`);
    }

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const price = opts.price ? adjust(opts.price, pPrice) : null;
    let type = inverseObj(ORDER_TYPE)[opts.type];

    if (opts.timeInForce === OrderTimeInForce.PostOnly && opts.type === OrderType.Limit) {
      type = 'POST_ONLY';
    }

    const req = omitUndefined({
      symbol: reverseSymbol(opts.symbol),
      order_type: type,
      order_price: opts.type === OrderType.Limit ? price : undefined,
      reduce_only: opts.reduceOnly,
      side: inverseObj(ORDER_SIDE)[opts.side],
      position_side: await this.getPositionSideForOrder(opts),
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);
    const payloads = times(lots, () => {
      return { ...req, order_quantity: lotSize };
    });

    if (rest) payloads.push({ ...req, order_quantity: rest });

    try {
      const orderIds = await mapSeries(payloads, async (payload) => {
        const { data } = await this.xhr.post<PLACE_ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDER, payload);
        return String(data.order_id);
      });

      if (opts.stopLoss || opts.takeProfit) {
        const algoIds = await this.placePositionalAlgoOrder({
          ...opts,
          side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        });
        orderIds.push(...algoIds);
      }

      return orderIds;
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  placeAlgoOrder = async (opts: PlaceOrderOpts) => {
    if (!opts.price) {
      this.emitAndLogError('Price is required for algo orders');
      return [];
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      return this.placeTrailingStopLossOrder(opts);
    }

    if (opts.amount) {
      return this.placeStopMarket(opts);
    }

    const params = {
      ...opts,
      symbol: opts.symbol,
      side: opts.side === OrderSide.Buy ? OrderSide.Buy : OrderSide.Sell,
      stopLoss: 0,
      takeProfit: 0,
    };

    const sibling = this.store.orders.find(
      (o) =>
        o.symbol === opts.symbol &&
        o.side === opts.side &&
        o.type === (opts.type === OrderType.StopLoss ? OrderType.TakeProfit : OrderType.StopLoss) &&
        o.status === OrderStatus.Open,
    );

    if (opts.type === OrderType.StopLoss) {
      params.stopLoss = opts.price;
    }

    if (opts.type === OrderType.TakeProfit) {
      params.takeProfit = opts.price;
    }

    if (sibling) {
      if (sibling.type === OrderType.StopLoss) params.stopLoss = sibling.price;
      if (sibling.type === OrderType.TakeProfit) params.takeProfit = sibling.price;
      await this.cancelAlgoOrder(sibling, false);
    }

    return await this.placePositionalAlgoOrder(params);
  };

  placeStopMarket = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickersBySymbol[opts.symbol];

    if (!market) throw new Error(`Market ${opts.symbol} not found`);
    if (!ticker) throw new Error(`Ticker ${opts.symbol} not found`);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const price = opts.price ? adjust(opts.price, pPrice) : null;

    const rawTriggerType =
      (opts.type === OrderType.StopLoss || opts.type === OrderType.TrailingStopLoss
        ? opts.slTriggerType
        : opts.tpTriggerType) || OrderStopTriggerType.MarkPrice;

    const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

    const req = omitUndefined({
      symbol: reverseSymbol(opts.symbol),
      type: 'MARKET',
      algoType: 'STOP',
      side: inverseObj(ORDER_SIDE)[opts.side],
      triggerPrice: price,
      triggerPriceType: triggerType,
      reduceOnly: true,
      positionSide: await this.getPositionSideForOrder({
        ...opts,
        reduceOnly: true,
      }),
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);
    const payloads = times(lots, () => {
      return { ...req, quantity: lotSize };
    });

    if (rest) payloads.push({ ...req, quantity: rest });

    try {
      const orderIds = await mapSeries(payloads, async (payload) => {
        const {
          data: { data },
        } = await this.xhr.post<PLACE_ALGO_ORDER_RESPONSE>(ENDPOINTS.ALGO_ORDER, payload);

        return data.rows.map((row) => String(row.orderId));
      });
      return orderIds.flat();
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  placeTrailingStopLossOrder = async (opts: PlaceOrderOpts) => {
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

    const rawTriggerType = opts.tpTriggerType || OrderStopTriggerType.MarkPrice;
    const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

    const payload = omitUndefined({
      symbol: reverseSymbol(opts.symbol),
      type: 'MARKET',
      algoType: 'TRAILING_STOP',
      callbackValue: `${priceDistance}`,
      side: inverseObj(ORDER_SIDE)[opts.side],
      reduceOnly: true,
      quantity: `${position.contracts}`,
      triggerPriceType: triggerType,
      positionSide: await this.getPositionSideForOrder({
        ...opts,
        reduceOnly: true,
      }),
    });

    try {
      const {
        data: { data },
      } = await this.xhr.post<{
        data: { rows: Array<Record<string, any>> };
      }>(ENDPOINTS.ALGO_ORDER, payload);

      return data?.rows?.map((r) => String(r.orderId)) || [];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  placePositionalAlgoOrder = async (
    opts: Pick<PlaceOrderOpts, 'side' | 'stopLoss' | 'symbol' | 'takeProfit' | 'tpTriggerType' | 'slTriggerType'>,
  ) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }
    const pPrice = market.precision.price;
    const otherProps = omitUndefined({
      reduceOnly: true,
      positionSide: await this.getPositionSideForOrder({
        ...opts,
        reduceOnly: true,
      }),
    });

    const req: Record<string, any> = {
      ...otherProps,
      symbol: reverseSymbol(opts.symbol),
      algoType: 'POSITIONAL_TP_SL',
      childOrders: [],
    };

    if (opts.stopLoss) {
      const rawTriggerType = opts.slTriggerType || OrderStopTriggerType.MarkPrice;
      const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

      req.childOrders.push({
        ...otherProps,
        algoType: 'STOP_LOSS',
        type: 'CLOSE_POSITION',
        triggerPriceType: triggerType,
        side: opts.side === OrderSide.Sell ? 'SELL' : 'BUY',
        triggerPrice: `${adjust(opts.stopLoss, pPrice)}`,
      });
    }

    if (opts.takeProfit) {
      const rawTriggerType = opts.tpTriggerType || OrderStopTriggerType.MarkPrice;
      const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

      req.childOrders.push({
        ...otherProps,
        algoType: 'TAKE_PROFIT',
        type: 'CLOSE_POSITION',
        triggerPriceType: triggerType,
        side: opts.side === OrderSide.Sell ? 'SELL' : 'BUY',
        triggerPrice: `${adjust(opts.takeProfit, pPrice)}`,
      });
    }

    try {
      const {
        data: { data },
      } = await this.xhr.post<{
        data: { rows: Array<Record<string, any>> };
      }>(ENDPOINTS.ALGO_ORDER, req);

      return data?.rows?.map((r) => String(r.orderId)) || [];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  cancelAllOrders = async () => {
    try {
      await this.xhr.delete(ENDPOINTS.CANCEL_ORDERS);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      const wooSymbol = reverseSymbol(symbol);
      await this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
        params: { symbol: wooSymbol },
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    // const positionMode = await this.getPositionMode();
    try {
      //   if (positionMode === PositionMode.OneWay) {
      await this.xhr.post(ENDPOINTS.FUTURES_LEVERAGE, {
        symbol: reverseSymbol(symbol),
        margin_mode: 'CROSS',
        position_side: 'BOTH',
        leverage: inputLeverage,
      });
      // } else {
      //   // const sides = side ? [side === PositionSide.Long ? 'LONG' : 'SHORT'] : ['LONG', 'SHORT'];
      //   // WOO seems to actually ignore this and always sets both, so to avoid regression lets just set both explicitly
      //   const sides = ['LONG', 'SHORT'];
      //   await mapSeries(sides, async (side) => {
      //     await this.xhr.post(ENDPOINTS.FUTURES_LEVERAGE, {
      //       symbol: reverseSymbol(symbol),
      //       margin_mode: 'CROSS',
      //       position_side: side,
      //       leverage: inputLeverage,
      //     });
      //   });
      // }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  changePositionMode = async (hedged: boolean) => {
    try {
      // ONE_WAY | HEDGE_MODE
      await this.xhr.post(ENDPOINTS.POSITION_MODE, { position_mode: hedged ? 'HEDGE_MODE' : 'ONE_WAY' });
      this._accountPositionMode = hedged ? PositionMode.Hedged : PositionMode.OneWay;
      const positions = await this.fetchPositions();
      this.store.update({ positions });
    } catch (e: any) {
      // {
      //   "success": false,
      //   "code": -1005,
      //   "error_code": 111000,
      //   "message": "Position mode cannot be altered while there are pending orders."
      // }
      this.emitAndLogError(e, e?.response?.data?.message || e?.message);
      throw e;
    }
  };

  private isAlgoOrder = (orderType: OrderType) => {
    return (
      orderType === OrderType.StopLoss || orderType === OrderType.TakeProfit || orderType === OrderType.TrailingStopLoss
    );
  };
}
