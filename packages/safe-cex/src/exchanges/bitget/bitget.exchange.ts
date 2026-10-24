import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import groupBy from 'lodash/groupBy';
import omit from 'lodash/omit';
import partition from 'lodash/partition';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';
import { mapSeries } from 'p-iteration';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
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
  timeframeToSeconds,
  UpdateOrderOpts,
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust, multiply, subtract } from '../../utils/safe-math';
import { splitTimeRange } from '../../utils/splitTimeRange';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { patternClientOrderIdRule } from '../clientOrderId';
import { paginateIncomeRange } from '../income-paging';
import { createAPI, isBitgetUtaStrategyOrderNotFound } from './bitget.api';
import { mapBitgetFills, mapBitgetUtaFills } from './bitget.fills';
import { mapBitgetBills } from './bitget.income';
import {
  BITGET_UTA_CATEGORY,
  BITGET_UTA_ORDER,
  BITGET_UTA_POSITION,
  ENDPOINTS,
  FETCH_POSITIONS_V2_RESPONSE,
  FETCH_SINGLE_ACCOUNT_RESPONSE,
  FETCH_UTA_ACCOUNT_ASSETS_RESPONSE,
  FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE,
  FETCH_UTA_FILLS_RESPONSE,
  FETCH_UTA_ORDERS_RESPONSE,
  FETCH_UTA_POSITIONS_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
  TIME_IN_FORCE,
} from './bitget.types';
import { BitgetPrivateWebsocket } from './bitget.ws-private';
import { BitgetPublicWebsocket } from './bitget.ws-public';
import { BitgetSecondaryPublicWebsocket } from './bitget.ws-secondary-public';

/**
 * Bitget caps a bill page at 100 rows. The window cap is not documented as a hard reject, so
 * a conservative 30-day chunk is used; the pager does not depend on either being exact —
 * it walks until the records span the request, so a narrower real window costs extra
 * requests rather than losing rows.
 */
const BILL_PAGE_LIMIT = 100;
/** Bitget caps a fills page at 100. */
const BITGET_FILLS_PAGE_LIMIT = 100;
/** Bitget REJECTS a fills window wider than 90 days — it does not clamp it. */
const BITGET_FILLS_MAX_WINDOW_MS = 89 * 24 * 60 * 60 * 1000;
/** Bitget UTA fill history rejects windows wider than 30 days. Use a sub-30-day inclusive span. */
const BITGET_UTA_FILLS_MAX_WINDOW_MS = 30 * 24 * 60 * 60 * 1000 - 1;
/** Backstop only; the cursor is what actually terminates the walk. */
const BITGET_FILLS_MAX_PAGES = 100;
const BILL_MAX_WINDOW_MS = 30 * 24 * 60 * 60 * 1000;
/** Bitget rejects candle requests whose paired start/end range reaches the documented 90-day edge. */
const BITGET_CANDLE_MAX_WINDOW_MS = 89 * 24 * 60 * 60 * 1000;
const BITGET_PRIVATE_STREAM_WRITE_GRACE_MS = 8_000;
const BITGET_PRIVATE_STREAM_DEGRADED_RECONCILE_MS = 10_000;
const BITGET_PRIVATE_STREAM_DEGRADED_MESSAGE =
  'Bitget private WS is not receiving live order/position updates. Tealstreet will reconcile through REST; if this account uses IP whitelisting, verify the terminal-listed browser/server WebSocket IPs are allowed.';

export const BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE =
  'Bitget UTA routing is available for USDT-M and USDC-M only; this market variant is not enabled for UTA yet.';

const isBitgetSuccessCode = (code: unknown): boolean => parseInt(`${code ?? 0}`, 10) === 0;

const isBitgetUtaClassicUnsupported = (payload?: { code?: string | number; msg?: string }): boolean => {
  const code = `${payload?.code ?? ''}`;
  const message = payload?.msg?.toLowerCase() ?? '';
  return (
    code === '40085' ||
    (message.includes('unified account mode') && message.includes('classic account api is not supported'))
  );
};

const getBitgetApiErrorMessage = (payload: { code?: string | number; msg?: string } | undefined, fallback: string) => {
  return payload?.msg || fallback;
};

const isBitgetEnvironmentMismatch = (message?: string): boolean =>
  message?.toLowerCase().includes('environment is incorrect') ?? false;

export class BitgetExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Bitget', /^[.A-Z:/a-z0-9_-]{1,32}$/);

  /** Journal records: this venue has no reported closed axis, so round trips fold from fills. */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) =>
    this.foldClosedPositionRecords(opts, { symbolScoped: false });

  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BitgetPublicWebsocket;
  secondsTickersWebsocket: BitgetSecondaryPublicWebsocket;
  thirdTickersWebsocket: BitgetSecondaryPublicWebsocket;
  privateWebsocket: BitgetPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // UTA vs Classic detection: v2 set-leverage couples long+short on Bitget; only
  // v3 honors per-side via `posSide`, but v3 endpoints return 40084 "Classic
  // Account mode" on non-UTA accounts. We can't distinguish UTA from Classic
  // from BALANCE/account responses (`assetMode` and `unionAvailable` are
  // populated on Classic too). Set definitively by one shared account-settings
  // probe during start/validation, or lazily before a private v2/v3 branch on
  // unstarted instances. Every v3-routing method reads this flag — keep it the
  // single source of truth.
  protected isUta = false;
  protected supportsUtaRouting = true;
  private _accountTypeDetected = false;
  private _accountTypeDetectionInflight: Promise<void> | null = null;

  get usesUtaRouting(): boolean {
    return this.isUta && this.supportsUtaRouting && !this.options.testnet;
  }

  // Short-lived dedup + cache for resolveSymbolPositioning's account-endpoint
  // fallback. A single order resolves BOTH position-mode (in formatCreateOrder)
  // and margin-type (in placeOrderBatch), and placeOrders() runs many
  // formatCreateOrder() in parallel — each fallback hits the 3-RPS-rate-limited
  // account endpoint. In-flight dedup collapses concurrent calls per symbol; the
  // short TTL lets the mode+margin pair reuse one fetch. Only successful (both
  // fields present) results are cached so a transient failure isn't sticky.
  private _positioningCache: Record<
    string,
    { at: number; value: { positionMode?: PositionMode; marginType?: MarginType } }
  > = {};
  private _positioningInflight: Record<string, Promise<{ positionMode?: PositionMode; marginType?: MarginType }>> = {};
  private static POSITIONING_CACHE_TTL_MS = 5_000;

  private _planTypePositionsCache: { at: number; value: Position[] } | null = null;
  private _planTypePositionsInflight: Promise<Position[]> | null = null;
  private _suppressBitgetValidationErrorEmission = false;
  private privateStreamWriteCheckTimeout: NodeJS.Timeout | undefined;
  private privateStreamDegradedReconcileTimeout: NodeJS.Timeout | undefined;
  private privateStreamDegradedReconcileRunning = false;
  private isPrivateStreamDegraded = false;
  private lastPrivateStreamMessageAt = 0;
  private hasWarnedPrivateStreamDegraded = false;
  /** Only the instance running the private stream (never the web's main-thread mirror) can judge it. */
  private privateStreamStarted = false;

  get suppressBitgetValidationErrorEmission(): boolean {
    return this._suppressBitgetValidationErrorEmission;
  }

  // private leverageHash: Record<string, number> = {};

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BitgetPublicWebsocket(this);
    this.privateWebsocket = new BitgetPrivateWebsocket(this);
    this.secondsTickersWebsocket = new BitgetSecondaryPublicWebsocket(this, 1);
    this.thirdTickersWebsocket = new BitgetSecondaryPublicWebsocket(this, 2);
  }

  get apiProductType() {
    return this.options.testnet ? 'susdt-futures' : 'usdt-futures';
  }

  get apiMarginCoin() {
    return this.options.testnet ? 'SUSDT' : 'USDT';
  }

  get apiUtaCategory(): BITGET_UTA_CATEGORY {
    return this.apiProductType.toUpperCase() as BITGET_UTA_CATEGORY;
  }

  private toFiniteNumber(value: unknown, fallback = 0): number {
    if (typeof value === 'number') {
      return Number.isFinite(value) ? value : fallback;
    }
    if (typeof value === 'string') {
      if (value.trim() === '') return fallback;
      const parsed = Number.parseFloat(value);
      return Number.isFinite(parsed) ? parsed : fallback;
    }
    return fallback;
  }

  private firstFiniteNumber(values: unknown[], fallback = 0): number {
    for (const value of values) {
      const parsed = this.toFiniteNumber(value, NaN);
      if (Number.isFinite(parsed)) return parsed;
    }
    return fallback;
  }

  private normalizeBitgetSymbol(value: unknown): string {
    const rawSymbol = String(value ?? '');
    const productSuffix = this.apiProductType.toUpperCase();
    const strippedSymbol = rawSymbol
      .replace(new RegExp(`_${productSuffix}$`, 'i'), '')
      .replace(/_(?:S?UMCBL|DMCBL|CMCBL)$/i, '');
    return this._resolveMarket(strippedSymbol)?.symbol ?? strippedSymbol;
  }

  private normalizeBitgetPositionMode(value: Record<string, any>): PositionMode {
    const rawMode = String(value.posMode ?? value.holdMode ?? '').toLowerCase();
    return rawMode === 'hedge_mode' || rawMode === 'double_hold' ? PositionMode.Hedged : PositionMode.OneWay;
  }

  private normalizeBitgetMarginType(value: unknown): MarginType {
    const rawMarginType = String(value ?? '').toLowerCase();
    return rawMarginType === 'crossed' || rawMarginType === 'cross' ? MarginType.Cross : MarginType.Isolated;
  }

  private normalizeBitgetPositionSide(value: Record<string, any>): PositionSide {
    const rawSide = String(value.holdSide ?? value.posSide ?? value.side ?? '').toLowerCase();
    if (rawSide === 'short' || rawSide === 'sell') return PositionSide.Short;
    return PositionSide.Long;
  }

  calculateUpnlFromLastPrice(positions: Position[] = this.store.positions): number {
    return positions.reduce((acc, position) => {
      const ticker = this.store.tickersBySymbol[position.symbol];
      const contracts = Math.abs(this.toFiniteNumber(position.contracts));
      const entryPrice = this.toFiniteNumber(position.entryPrice);
      const apiUnrealizedPnl = this.toFiniteNumber(position.unrealizedPnl);

      if (contracts === 0) {
        return acc;
      }

      if (!ticker || this.toFiniteNumber(ticker.last) === 0) {
        return acc + apiUnrealizedPnl;
      }

      if (position.side !== PositionSide.Long && position.side !== PositionSide.Short) {
        return acc + apiUnrealizedPnl;
      }

      const lastPrice = this.toFiniteNumber(ticker.last);
      const direction = position.side === PositionSide.Long ? 1 : -1;
      const recalculatedUpnl = direction * (lastPrice - entryPrice) * contracts;

      return acc + this.toFiniteNumber(recalculatedUpnl, apiUnrealizedPnl);
    }, 0);
  }

  calculateAccountUpnl = (apiUpnl: number): number => {
    const normalizedApiUpnl = this.toFiniteNumber(apiUpnl);
    const hasOpenPositions = this.store.positions.some(
      (position) => Math.abs(this.toFiniteNumber(position.contracts)) > 0,
    );

    if (!hasOpenPositions) {
      return normalizedApiUpnl;
    }

    return this.calculateUpnlFromLastPrice(this.store.positions);
  };

  getAccount = async () => {
    const {
      data: { data },
    } = await this.xhr.get(ENDPOINTS.ACCOUNT_SPOT);

    return {
      userId: `${data.parentId || data.userId}`,
      subId: data.parentId ? `${data.userId}` : null,
      affiliateId: data.inviterId ? `${data.inviterId}` : null,
    };
  };

  validateAccount = async () => {
    const previousSuppressValidationErrorEmission = this._suppressBitgetValidationErrorEmission;
    this._suppressBitgetValidationErrorEmission = true;
    try {
      await this.ensureAccountTypeDetected();
      // fetchBalance hits the BALANCE endpoint (validating the key) and throws on
      // auth failure, so it doubles as the validation call — one round-trip.
      const balance = await this.fetchBalance();
      return { error: '', balance };
    } catch (err: any) {
      const errorMsg = getBitgetApiErrorMessage(err?.response?.data, err?.message || 'Invalid API key or secret');

      // Detect environment mismatch and provide helpful message
      if (isBitgetEnvironmentMismatch(errorMsg)) {
        const currentMode = this.options.testnet ? 'Demo/Testnet' : 'Live/Production';
        const suggestedMode = this.options.testnet ? 'Live/Production' : 'Demo/Testnet';
        const helpfulMsg = `API key environment mismatch: Your API key appears to be for ${suggestedMode} trading, but Tealstreet is configured for ${currentMode}. Please check your Testnet setting.`;
        return { error: helpfulMsg };
      }

      return { error: errorMsg || 'Invalid API key or secret' };
    } finally {
      this._suppressBitgetValidationErrorEmission = previousSuppressValidationErrorEmission;
    }
  };

  dispose = () => {
    super.dispose();
    if (this.privateStreamWriteCheckTimeout) {
      clearTimeout(this.privateStreamWriteCheckTimeout);
      this.privateStreamWriteCheckTimeout = undefined;
    }
    if (this.privateStreamDegradedReconcileTimeout) {
      clearTimeout(this.privateStreamDegradedReconcileTimeout);
      this.privateStreamDegradedReconcileTimeout = undefined;
    }
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
    this.secondsTickersWebsocket.dispose();
    this.thirdTickersWebsocket.dispose();
  };

  markPrivateStreamMessageReceived = () => {
    this.isPrivateStreamDegraded = false;
    this.lastPrivateStreamMessageAt = Date.now();
    this.hasWarnedPrivateStreamDegraded = false;
    this.store.updateSocketHealth({ privateDegradedReason: undefined });
    this.stopPrivateStreamDegradedReconcile();
  };

  private markPrivateStreamDegraded = (reason = BITGET_PRIVATE_STREAM_DEGRADED_MESSAGE, forceToast = false) => {
    this.isPrivateStreamDegraded = true;
    this.store.updateSocketHealth({ privateDegradedReason: reason });
    this.log(reason, LogSeverity.Warning);
    this.privateWebsocket.recoverMissingPrivateStream(reason);
    this.startPrivateStreamDegradedReconcile();
    if (forceToast && !this.hasWarnedPrivateStreamDegraded) {
      this.hasWarnedPrivateStreamDegraded = true;
      this.emitter.emit('warning', reason, { forceToast: true });
    }
  };

  private startPrivateStreamDegradedReconcile = () => {
    if (this.privateStreamDegradedReconcileRunning || this.privateStreamDegradedReconcileTimeout) {
      return;
    }
    void this.reconcilePrivateStateFromRest();
  };

  private stopPrivateStreamDegradedReconcile = () => {
    if (this.privateStreamDegradedReconcileTimeout) {
      clearTimeout(this.privateStreamDegradedReconcileTimeout);
      this.privateStreamDegradedReconcileTimeout = undefined;
    }
  };

  private schedulePrivateStreamDegradedReconcile = () => {
    if (this.isDisposed || !this.isPrivateStreamDegraded || this.privateStreamDegradedReconcileTimeout) {
      return;
    }

    this.privateStreamDegradedReconcileTimeout = setTimeout(() => {
      this.privateStreamDegradedReconcileTimeout = undefined;
      void this.reconcilePrivateStateFromRest();
    }, BITGET_PRIVATE_STREAM_DEGRADED_RECONCILE_MS);
  };

  private reconcilePrivateStateFromRest = async () => {
    if (this.isDisposed || !this.isPrivateStreamDegraded || this.privateStreamDegradedReconcileRunning) {
      return;
    }

    this.privateStreamDegradedReconcileRunning = true;
    try {
      const [positionsResult, ordersResult, balanceResult] = await Promise.allSettled([
        this.fetchPositions(),
        this.fetchOrders(),
        this.fetchBalance({ preferFast: true }),
      ]);

      if (this.isDisposed || !this.isPrivateStreamDegraded) {
        return;
      }

      const loaded = { ...this.store.loaded };
      const update: Record<string, unknown> = {};

      if (positionsResult.status === 'fulfilled') {
        this.store.addOrUpdatePositionsFull(positionsResult.value);
        loaded.positions = true;
      }

      if (ordersResult.status === 'fulfilled') {
        update.orders = ordersResult.value;
        loaded.orders = true;
      }

      if (balanceResult.status === 'fulfilled') {
        update.balance = balanceResult.value;
        loaded.balance = true;
      }

      this.store.update({
        ...update,
        loaded,
      });
    } finally {
      this.privateStreamDegradedReconcileRunning = false;
      this.schedulePrivateStreamDegradedReconcile();
    }
  };

  private schedulePrivateStreamWriteCheck = () => {
    // The web's main-thread mirror places UI orders but never connects the private
    // stream, so a watchdog there always fired: a false toast, a REST loop writing
    // the UI store, and a second private socket opened from the mirror.
    if (!this.privateStreamStarted) return;
    const lastPrivateFrameAtSchedule = this.lastPrivateStreamMessageAt;
    if (this.privateStreamWriteCheckTimeout) {
      clearTimeout(this.privateStreamWriteCheckTimeout);
    }

    this.privateStreamWriteCheckTimeout = setTimeout(() => {
      this.privateStreamWriteCheckTimeout = undefined;
      if (this.isDisposed || this.lastPrivateStreamMessageAt > lastPrivateFrameAtSchedule) {
        return;
      }

      this.markPrivateStreamDegraded(BITGET_PRIVATE_STREAM_DEGRADED_MESSAGE, true);
    }, BITGET_PRIVATE_STREAM_WRITE_GRACE_MS);
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      // start public websocket streams as soon as markets loaded.
      this.publicWebsocket.connectAndSubscribe();

      // loop fetch leverage brackets
      // this.fetchLeverageBrackets();

      // loop fetch positions leverage
      // this.fetchCurrentLeverage();

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Bitget markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      // start additional public ticker websocket streams
      this.secondsTickersWebsocket.connectAndSubscribe();
      this.thirdTickersWebsocket.connectAndSubscribe();
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    await this.ensureAccountTypeDetected();
    if (this.isDisposed) return;

    this.privateStreamStarted = true;
    this.privateWebsocket.connectAndSubscribe();

    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Bitget`);

    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded Bitget orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.store.addOrUpdatePositionsFull(positions);

        let balance;
        if (count % 2 === 0) {
          balance = await this.fetchBalance();
          if (this.isDisposed) return;
        }

        // virtual TPSL orders on limits dont stream via ws
        // bitget caches rest responses for a few seconds so best to not overwrite limit orders here as
        // ws updates are newer...
        let orders;
        if (count % 4 === 0) {
          const fetchedNormalOrders = await this.fetchNormalOrders();
          const virtualTPSLOrders = fetchedNormalOrders.filter(
            (o) => o.type === OrderType.StopLoss || o.type === OrderType.TakeProfit,
          );
          const existingNonVirtualTPSLOrders = this.store.orders.filter(
            (o) => !o.id.endsWith('__stop_loss') && !o.id.endsWith('__take_profit'),
          );
          orders = [...existingNonVirtualTPSLOrders, ...virtualTPSLOrders];
        }

        this.store.update({
          ...(balance && { balance }),
          ...(orders && { orders }),
          loaded: {
            ...this.store.loaded,
            ...(balance && { balance: true }),
            positions: true,
            ...(orders && { orders: true }),
          },
        });
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), this.getTickInterval());
      }
    }
  };

  // One-time probe: GET a cheap v3 endpoint and observe whether Bitget routes
  // the call (UTA) or returns 40084 "Classic Account mode" (Classic). Result
  // is sticky for the worker's lifetime; transport-only probe failures leave
  // `isUta=false` so we degrade safely to v2. Auth/environment errors are
  // terminal validation failures and must not fall through to Classic.
  private _detectAccountType = async (): Promise<void> => {
    let shouldPropagateProbeError = false;
    let detected = false;
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_SETTINGS_V3);
      const code = `${data?.code ?? ''}`;
      if (isBitgetSuccessCode(data?.code)) {
        this.isUta = true;
        detected = true;
      } else if (code === '40084') {
        this.isUta = false;
        detected = true;
      } else {
        shouldPropagateProbeError = true;
        throw new Error(getBitgetApiErrorMessage(data, 'Bitget account type probe failed'));
      }
    } catch (err: any) {
      if (shouldPropagateProbeError) {
        throw err;
      }

      const code = `${err?.response?.data?.code ?? ''}`;
      if (code === '40084') {
        this.isUta = false;
        detected = true;
      } else if (err?.response?.data?.code || isBitgetEnvironmentMismatch(err?.response?.data?.msg)) {
        throw err;
      } else {
        // Network or transient error — leave default (false) so we don't
        // mistakenly route Classic accounts to v3.
        this.log(`Bitget account type probe failed; assuming Classic: ${err?.message ?? err}`);
        detected = true;
      }
    } finally {
      this._accountTypeDetected = detected;
    }
  };

  private ensureAccountTypeDetected = async (): Promise<void> => {
    if (this.isPublicDataOnly || this._accountTypeDetected) return;
    if (!this._accountTypeDetectionInflight) {
      this._accountTypeDetectionInflight = this._detectAccountType().finally(() => {
        this._accountTypeDetectionInflight = null;
      });
    }
    return this._accountTypeDetectionInflight;
  };

  private promoteUtaRoutingFromClassicUnsupported = (payload?: { code?: string | number; msg?: string }): boolean => {
    if (!isBitgetUtaClassicUnsupported(payload) || !this.supportsUtaRouting || this.options.testnet) {
      return false;
    }

    this.isUta = true;
    this._accountTypeDetected = true;
    return true;
  };

  fetchBalance = async ({ preferFast = false }: { preferFast?: boolean } = {}) => {
    await this.ensureAccountTypeDetected();
    const client = preferFast ? this.unlimitedXHR : this.xhr;
    if (this.usesUtaRouting) {
      return this.fetchUtaBalance(client);
    }

    const { data: response } = await client.get<{
      code?: string | number;
      msg?: string;
      data: Array<Record<string, any>> | null;
    }>(ENDPOINTS.BALANCE, {
      params: { productType: this.apiProductType },
    });

    if (!isBitgetSuccessCode(response.code)) {
      if (this.promoteUtaRoutingFromClassicUnsupported(response)) {
        return this.fetchUtaBalance(client);
      }
      throw new Error(getBitgetApiErrorMessage(response, 'Bitget account balance request failed'));
    }

    const data = Array.isArray(response.data) ? response.data : [];
    const usdt = data.find((b) => b.marginCoin === this.apiMarginCoin);
    if (!usdt) return this.store.balance;

    // {
    //   "used": null,
    //   "free": 347.2238322,
    //   "total": 347.2238322,
    //   "upnl": 8.349379922,
    //   "info": {
    //     "marginCoin": "USDT",
    //     "locked": "11.24980215",
    //     "available": "347.2238322",
    //     "crossedMaxAvailable": "337.46804868",
    //     "isolatedMaxAvailable": "337.46804868",
    //     "maxTransferOut": "329.11866876",
    //     "accountEquity": "355.57321213",
    //     "usdtEquity": "355.573212130794",
    //     "btcEquity": "0.00328947755",
    //     "crossedRiskRate": "0.015549806351",
    //     "unrealizedPL": "8.349379922",
    //     "coupon": "0",
    //     "crossedUnrealizedPL": "",
    //     "isolatedUnrealizedPL": "",
    //     "grant": "0",
    //     "unionTotalMargin": "355.57321213",
    //     "unionAvailable": "337.46804868",
    //     "unionMm": "5.5290946",
    //     "assetList": [],
    //     "assetMode": "single",
    //     "isolatedMargin": "0",
    //     "crossedMargin": "13.5547525"
    //   }

    const accountEquity = this.toFiniteNumber(usdt.unionTotalMargin, this.toFiniteNumber(usdt.accountEquity));
    const available = this.toFiniteNumber(usdt.unionAvailable, this.toFiniteNumber(usdt.available));
    const apiUpnl = this.toFiniteNumber(
      usdt.unrealizedPL,
      this.toFiniteNumber(usdt.crossedUnrealizedPL) + this.toFiniteNumber(usdt.isolatedUnrealizedPL),
    );

    // Bitget's account-level unrealizedPL can intermittently be empty/stale.
    // Recalculate from current positions + last price for margin UI consistency.
    const upnl = this.calculateAccountUpnl(apiUpnl);

    const balance: Balance = {
      used: subtract(accountEquity, available),
      free: available,
      total: subtract(accountEquity, upnl),
      upnl,
      info: usdt,
    };

    return balance;
  };

  private fetchUtaBalance = async (client: Axios): Promise<Balance> => {
    const { data: response } = await client.get<FETCH_UTA_ACCOUNT_ASSETS_RESPONSE>(ENDPOINTS.UTA_ACCOUNT_ASSETS);

    if (!isBitgetSuccessCode(response.code)) {
      throw new Error(getBitgetApiErrorMessage(response, 'Bitget UTA account assets request failed'));
    }

    const data = response.data;
    if (!data) return this.store.balance;

    const asset = data.assets?.find((a) => a.coin === this.apiMarginCoin);
    const accountEquity = this.toFiniteNumber(
      data.accountEquity,
      this.toFiniteNumber(data.usdtEquity, this.toFiniteNumber(asset?.usdValue, this.toFiniteNumber(asset?.equity))),
    );
    const available = this.toFiniteNumber(data.effEquity, this.toFiniteNumber(asset?.available));
    const apiUpnl = this.toFiniteNumber(data.unrealisedPnl, this.toFiniteNumber(data.usdtUnrealisedPnl));
    const upnl = this.calculateAccountUpnl(apiUpnl);

    return {
      used: subtract(accountEquity, available),
      free: available,
      total: subtract(accountEquity, upnl),
      upnl,
      info: data,
    };
  };

  fetchPositionMetadata = async (
    symbols: string[],
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this.fetchUtaPositionMetadata(symbols);
    }

    try {
      return flatten(
        await mapSeries(
          symbols,
          async (symbol): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
            if (this.isDisposed) return [];

            const productType = this.apiProductType;
            const marginCoin = this.apiMarginCoin;

            const { data } = await this.xhr.get<FETCH_SINGLE_ACCOUNT_RESPONSE>(ENDPOINTS.FETCH_SINGLE_ACCOUNT, {
              params: {
                symbol: symbol,
                productType,
                marginCoin,
              },
            });

            if (this.isDisposed) return [];

            if (parseInt(data.code || '0') !== 0) {
              this.emitAndLogError(data.msg);
              return [];
            }

            const response = data.data;
            const positionMode = response.posMode === 'hedge_mode' ? PositionMode.Hedged : PositionMode.OneWay;
            const marginType = response.marginMode === 'crossed' ? MarginType.Cross : MarginType.Isolated;

            const buyIsolatedLeverage = parseInt(response.isolatedLongLever);
            const sellIsolatedLeverage = parseInt(response.isolatedShortLever);
            const crossLeverage = parseInt(response.crossedMarginLeverage);

            if (
              positionMode === PositionMode.OneWay &&
              marginType === MarginType.Isolated &&
              buyIsolatedLeverage !== sellIsolatedLeverage
            ) {
              this.emitAndLogError(`Mismatched one-way isolated leverages for ${symbol}. Please set them to be equal.`);
            }

            if (positionMode === PositionMode.OneWay) {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage: marginType === MarginType.Cross ? crossLeverage : buyIsolatedLeverage,
                  positionMode,
                  marginType,
                },
              ];
            } else {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage: marginType === MarginType.Cross ? crossLeverage : buyIsolatedLeverage,
                  positionMode,
                  marginType,
                },
                {
                  symbol,
                  side: PositionSide.Short,
                  leverage: marginType === MarginType.Cross ? crossLeverage : sellIsolatedLeverage,
                  positionMode,
                  marginType,
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

  private fetchUtaPositionMetadata = async (
    symbols: string[],
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
    try {
      const { data } = await this.xhr.get<FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE>(ENDPOINTS.ACCOUNT_SETTINGS_V3);
      if (!isBitgetSuccessCode(data.code)) {
        this.emitAndLogError(data.msg);
        return [];
      }

      const settings = data.data;
      if (!settings) return [];

      const positionMode = settings.holdMode === 'hedge_mode' ? PositionMode.Hedged : PositionMode.OneWay;
      const rows = settings.symbolConfigList ?? [];
      const metadata: Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[] = [];

      for (const symbol of symbols) {
        const market = this.store.marketsBySymbol[symbol];
        const venueSymbol = market?.id ?? symbol;
        const config = rows.find(
          (row) =>
            row.symbol === venueSymbol &&
            (!row.category || row.category.toUpperCase() === this.apiUtaCategory.toUpperCase()),
        );

        // Bitget documents `marginMode` as defaulting to cross when omitted on
        // UTA futures orders. Fresh/flat symbols can be absent from
        // symbolConfigList, but the account-level holdMode is still enough to
        // format side semantics.
        const marginType = config?.marginMode === 'isolated' ? MarginType.Isolated : MarginType.Cross;
        const longLeverage = this.toFiniteNumber(config?.longLeverage, this.toFiniteNumber(config?.leverage, 1));
        const shortLeverage = this.toFiniteNumber(config?.shortLeverage, this.toFiniteNumber(config?.leverage, 1));

        metadata.push({
          symbol,
          side: PositionSide.Long,
          leverage: longLeverage,
          positionMode,
          marginType,
        });

        if (positionMode === PositionMode.Hedged) {
          metadata.push({
            symbol,
            side: PositionSide.Short,
            leverage: shortLeverage,
            positionMode,
            marginType,
          });
        }
      }

      return metadata;
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchPositions = async (symbols?: string[]) => {
    const openPositions = await this.fetchOpenPositions();
    if (!symbols) return openPositions;
    const openPositionsForSymbols = openPositions.filter((p) => symbols.includes(p.symbol));
    const symbolsNeedingMetadata = symbols.filter((symbol) => {
      const symbolPositions = openPositionsForSymbols.filter((p) => p.symbol === symbol);
      if (symbolPositions.length === 0) return true;
      if (!symbolPositions.some((p) => p.positionMode === PositionMode.Hedged)) return false;

      const hasLong = symbolPositions.some((p) => p.side === PositionSide.Long);
      const hasShort = symbolPositions.some((p) => p.side === PositionSide.Short);
      return !hasLong || !hasShort;
    });
    const positionMetadatas = await this.fetchPositionMetadata(symbolsNeedingMetadata);
    const fakePositions = positionMetadatas
      .filter(
        (metadata) =>
          !openPositionsForSymbols.some(
            (position) =>
              position.symbol === metadata.symbol &&
              position.side === metadata.side &&
              position.positionMode === metadata.positionMode,
          ),
      )
      .map((metadata) => {
        return {
          id: createPositionId(metadata.symbol, metadata.side, metadata.positionMode),
          ...metadata,
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
        };
      });
    return uniqBy([...openPositionsForSymbols, ...fakePositions], (position) => position.id);
  };

  fetchOpenPositions = async () => {
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this.fetchUtaOpenPositions();
    }

    const { data } = await this.xhr.get<FETCH_POSITIONS_V2_RESPONSE>(ENDPOINTS.POSITIONS_V2, {
      params: {
        productType: this.apiProductType,
        marginCoin: this.apiMarginCoin,
      },
    });

    if (!isBitgetSuccessCode(data.code)) {
      if (this.promoteUtaRoutingFromClassicUnsupported(data)) {
        return this.fetchUtaOpenPositions();
      }
      // Detect environment mismatch and provide helpful message
      if (isBitgetEnvironmentMismatch(data.msg)) {
        const currentMode = this.options.testnet ? 'Demo/Testnet' : 'Live/Production';
        const suggestedMode = this.options.testnet ? 'Live/Production' : 'Demo/Testnet';
        this.emitAndLogError(
          `API key environment mismatch: Your API key is for ${suggestedMode}, but configured for ${currentMode}. Check Testnet setting.`,
        );
      } else {
        this.emitAndLogError(getBitgetApiErrorMessage(data, 'Bitget positions request failed'));
      }
      return [];
    }

    return data.data.map(this.mapPosition);
  };

  private fetchUtaOpenPositions = async (): Promise<Position[]> => {
    const { data } = await this.xhr.get<FETCH_UTA_POSITIONS_RESPONSE>(ENDPOINTS.UTA_POSITIONS, {
      params: {
        category: this.apiUtaCategory,
      },
    });

    if (!isBitgetSuccessCode(data.code)) {
      this.emitAndLogError(getBitgetApiErrorMessage(data, 'Bitget UTA positions request failed'));
      return [];
    }

    const rawPositions = Array.isArray(data.data?.list) ? data.data.list : [];
    return rawPositions.map(this.mapUtaPosition);
  };

  protected resolveLiquidationPrice = (positionId: string, rawLiquidationPrice: unknown): number => {
    const parsedLiquidationPrice = parseFloat(String(rawLiquidationPrice ?? ''));
    if (Number.isFinite(parsedLiquidationPrice)) return parsedLiquidationPrice;
    return this.store.positions.find((position) => position.id === positionId)?.liquidationPrice || 0;
  };

  /**
   * Resolve market from symbol, handling various symbol formats.
   * Supports: BTCUSDT (native), BTC-PERP, BTC-USDT, etc.
   */
  private _resolveMarket(symbol: string): Market | undefined {
    // Try direct lookup first
    let market = this.store.marketsBySymbol[symbol];
    if (market) return market;

    // Normalize symbol: remove common suffixes and separators
    const normalized = symbol.toUpperCase().replace(/[-_]/g, '').replace(/PERP$/, '').replace(/SWAP$/, '');

    // Try normalized symbol
    market = this.store.marketsBySymbol[normalized];
    if (market) return market;

    // Try adding USDT suffix if not present
    if (!normalized.endsWith('USDT') && !normalized.endsWith('USD')) {
      market = this.store.marketsBySymbol[`${normalized}USDT`];
      if (market) return market;
    }

    // Extract base currency and find by base + quote
    const base = normalized.replace(/USDT$/, '').replace(/USD$/, '').replace(/BUSD$/, '');

    // Find market with matching base and expected quote
    return this.store.markets.find((m) => m.base === base && m.quote === this.apiMarginCoin);
  }

  /**
   * One page of `/api/v2/mix/account/bill`, RETHROWING on failure.
   *
   * The public `fetchIncomeHistory` below swallows errors into `[]` for consistency with
   * every other venue, but a pager must never be handed that contract: a rate-limited page
   * and an empty window become the same value, so a 429 mid-walk reads as "nothing more
   * here" and truncates the ledger silently. Measured on binance, where exactly that lost 20
   * of 512 records while reporting success — so bitget is built with the split from the
   * start rather than retrofitted.
   */
  private _fetchBillPage = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) return [];

    const params: Record<string, any> = {
      productType: this.apiProductType,
      limit: Math.min(opts?.limit || BILL_PAGE_LIMIT, BILL_PAGE_LIMIT),
    };
    if (opts?.symbol) params.symbol = this._resolveMarket(opts.symbol)?.id ?? opts.symbol;
    if (opts?.startTime) params.startTime = opts.startTime;
    if (opts?.endTime) params.endTime = opts.endTime;

    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_BILL, { params });
    if (data?.code && String(data.code) !== '00000') {
      throw new Error(`${data.code} ${data.msg ?? ''}`.trim());
    }

    const rows: Array<Record<string, any>> = Array.isArray(data?.data?.bills) ? data.data.bills : [];
    return mapBitgetBills(rows);
  };

  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    try {
      return await this._fetchBillPage(opts);
    } catch (err: any) {
      this.emitAndLogError(`Error fetching income history: ${err?.message ?? err}`);
      return [];
    }
  };

  /**
   * Income over a range. Uses the shared span-based pager rather than a fourth copy — see
   * `exchanges/income-paging.ts` for why completeness is decided by span rather than by a
   * full-page row count, and why no sort order is assumed.
   *
   * Bitget's ordering is UNVERIFIED as a contract even though the sampled account answers
   * newest-first: this epic has twice been bitten by inferring a whole set from the first
   * page of a sorted list, most recently in the activity probe that wrongly called this very
   * account idle. The pager reads each page's order from the page itself.
   */
  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    const { records, truncated, exhaustedChunks } = await paginateIncomeRange(
      ({ startTime: from, endTime: to, limit }) => this._fetchBillPage({ startTime: from, endTime: to, limit }),
      {
        startTime,
        endTime,
        chunkMs: BILL_MAX_WINDOW_MS,
        pageLimit: BILL_PAGE_LIMIT,
      },
    );

    if (truncated) {
      this.log(
        `Income history for ${new Date(startTime).toISOString()}..${new Date(endTime).toISOString()} is INCOMPLETE — ` +
          `${exhaustedChunks} chunk(s) could not be walked to completion. Any total derived from it is a lower bound.`,
        LogSeverity.Warning,
      );
    }

    return incomeType ? records.filter((record) => record.incomeType === incomeType) : records;
  };

  /**
   * Per-fill executions from `/api/v2/mix/order/fills`.
   *
   * This used to read `orders-history` and filter `state === 'filled'` — an ORDER-level
   * route, so partial fills collapsed into a single row and the result carried no per-fill
   * id, no realized PnL, and no hedge leg. Without realized PnL the venue cannot fold
   * position records at all, which is what kept it fold-only-in-name and ungradeable.
   *
   * The per-fill route is also ACCOUNT-WIDE: unlike binance's `userTrades` it answers
   * without a symbol, so callers do not have to discover a symbol list first. A symbol is
   * still forwarded when given.
   */
  /** One cursor page of fills. Rethrows so a failed page can never read as an empty one. */
  private _fetchFillsPage = async (params: Record<string, any>): Promise<{ rows: any[]; endId?: string }> => {
    const { data } = await this.xhr.get(ENDPOINTS.ORDER_FILLS, { params });
    if (data?.code && String(data.code) !== '00000') {
      throw new Error(`${data.code} ${data.msg ?? ''}`.trim());
    }
    return {
      rows: Array.isArray(data?.data?.fillList) ? data.data.fillList : [],
      endId: data?.data?.endId ? String(data.data.endId) : undefined,
    };
  };

  /** One cursor page of UTA fills. Rethrows so a failed page can never read as an empty one. */
  private _fetchUtaFillsPage = async (params: Record<string, any>): Promise<{ rows: any[]; cursor?: string }> => {
    const { data } = await this.xhr.get<FETCH_UTA_FILLS_RESPONSE>(ENDPOINTS.UTA_ORDER_FILLS, { params });
    if (data?.code && !isBitgetSuccessCode(data.code)) {
      throw new Error(getBitgetApiErrorMessage(data, 'Bitget UTA fills request failed'));
    }

    return {
      rows: Array.isArray(data?.data?.list) ? data.data.list : [],
      cursor: data?.data?.cursor ? String(data.data.cursor) : undefined,
    };
  };

  private _fetchHistoricalUtaFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    const { symbol, orderId, from, to, limit = BITGET_FILLS_PAGE_LIMIT } = opts;

    let normalizedSymbol: string | undefined;
    if (symbol) {
      const market = this._resolveMarket(symbol);
      if (!market) return [];
      normalizedSymbol = market.symbol;
    }

    const deps = {
      resolveSymbol: (raw: string) => this._resolveMarket(raw)?.symbol,
      toTimestamp: (ms: number) => Math.floor(ms / 1000),
    };

    const windows =
      from !== undefined && to !== undefined ? splitTimeRange(from, to, BITGET_UTA_FILLS_MAX_WINDOW_MS) : [undefined];

    const executions: Execution[] = [];
    const seen = new Set<string>();

    try {
      for (const window of windows) {
        let cursor: string | undefined;

        for (let page = 0; page < BITGET_FILLS_MAX_PAGES; page += 1) {
          const params: Record<string, any> = {
            category: this.apiUtaCategory,
            limit: Math.min(limit, BITGET_FILLS_PAGE_LIMIT),
          };
          if (orderId) params.orderId = orderId;
          if (window) {
            params.startTime = window.startTime;
            params.endTime = window.endTime;
          }
          if (cursor) params.cursor = cursor;

          const { rows, cursor: nextCursor } = await this._fetchUtaFillsPage(params);
          if (!rows.length) break;

          for (const execution of mapBitgetUtaFills(rows, deps)) {
            if (normalizedSymbol && execution.symbol !== normalizedSymbol) continue;
            const id = execution.id ?? `${execution.symbol}|${execution.timestamp}|${execution.qty}|${execution.price}`;
            if (seen.has(id)) continue;
            seen.add(id);
            executions.push(execution);
          }

          // Terminate on cursor exhaustion or non-advancement, never on a short page.
          if (!nextCursor || nextCursor === cursor) break;
          cursor = nextCursor;
        }
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return executions;
    }

    return executions;
  };

  /**
   * Per-fill executions from `/api/v2/mix/order/fills`.
   *
   * This used to read `orders-history` and filter `state === 'filled'` — an ORDER-level
   * route, so partial fills collapsed into a single row and the result carried no per-fill
   * id, no realized PnL, and no hedge leg. Without realized PnL the venue cannot fold
   * position records at all, which is what kept it ungradeable.
   *
   * TWO THINGS THE OLD SINGLE-REQUEST SHAPE GOT WRONG, both already-known lessons here:
   *
   * 1. **The window is REJECTED, not clamped.** Bitget answers a range wider than 90 days
   *    with `00001 startTime and endTime interval cannot be greater than 90 days`, and the
   *    old catch turned that into `[]` — so an over-wide ask read as "this account has never
   *    traded". Identical to the binance/bybit defect `splitTimeRange` exists for, which is
   *    what is used here.
   * 2. **A page caps at 100 rows**, so one request silently truncated any active window.
   *    Paged on the venue's own `endId`/`idLessThan` cursor, terminating when the cursor
   *    stops advancing rather than when a page looks short — a short page is a guess about
   *    the venue, and this epic has been bitten by that guess twice.
   *
   * The route is also ACCOUNT-WIDE — unlike binance's `userTrades` it answers without a
   * symbol, so callers need not discover a symbol list first.
   */
  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) return [];
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this._fetchHistoricalUtaFills(opts);
    }

    const { symbol, from, to, limit = BITGET_FILLS_PAGE_LIMIT } = opts;

    let venueSymbol: string | undefined;
    if (symbol) {
      const market = this._resolveMarket(symbol);
      if (!market) return [];
      venueSymbol = market.id;
    }

    const deps = {
      resolveSymbol: (raw: string) => this._resolveMarket(raw)?.symbol,
      // Executions are stamped in SECONDS, matching the previous mapper and chart markers.
      toTimestamp: (ms: number) => Math.floor(ms / 1000),
    };

    const windows =
      from !== undefined && to !== undefined ? splitTimeRange(from, to, BITGET_FILLS_MAX_WINDOW_MS) : [undefined];

    const executions: Execution[] = [];
    const seen = new Set<string>();

    try {
      for (const window of windows) {
        let cursor: string | undefined;

        for (let page = 0; page < BITGET_FILLS_MAX_PAGES; page += 1) {
          const params: Record<string, any> = {
            productType: this.apiProductType,
            limit: Math.min(limit, BITGET_FILLS_PAGE_LIMIT),
          };
          if (venueSymbol) params.symbol = venueSymbol;
          if (window) {
            params.startTime = window.startTime;
            params.endTime = window.endTime;
          }
          if (cursor) params.idLessThan = cursor;

          const { rows, endId } = await this._fetchFillsPage(params);
          if (!rows.length) break;

          for (const execution of mapBitgetFills(rows, deps)) {
            const id = execution.id ?? `${execution.symbol}|${execution.timestamp}|${execution.qty}|${execution.price}`;
            if (seen.has(id)) continue;
            seen.add(id);
            executions.push(execution);
          }

          // Terminate on cursor exhaustion or non-advancement, never on a short page.
          if (!endId || endId === cursor) break;
          cursor = endId;
        }
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return executions;
    }

    return executions;
  };

  fetchMarkets = async () => {
    const {
      data: { data },
    } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.MARKETS, {
      params: { productType: this.apiProductType },
    });

    const markets: Market[] = data
      .filter((m) => m.quoteCoin === this.apiMarginCoin)
      .map((m) => {
        // Calculate price precision based on pricePlace and priceEndStep
        const priceDecimals = 10 / 10 ** (parseFloat(m.pricePlace) + 1);
        const pricePrecision = priceDecimals * parseFloat(m.priceEndStep);
        const maxOrderQty =
          parseFloat(m.maxOrderQty) || parseFloat(m.maxMarketOrderQty) || parseFloat(m.maxTradeNum) || Infinity;

        return {
          id: m.symbol,
          symbol: m.symbol,
          base: m.baseCoin,
          quote: m.quoteCoin,
          active: m.symbolStatus === 'normal' || m.symbolStatus === 'listed' || m.symbolStatus === 'limit_open',
          precision: {
            amount: parseFloat(m.sizeMultiplier),
            price: pricePrecision,
          },
          limits: {
            amount: {
              min: parseFloat(m.minTradeNum) || 0,
              max: maxOrderQty,
            },
            leverage: {
              min: parseFloat(m.minLever) || 1,
              max: parseFloat(m.maxLever) || 20,
            },
          },
          info: m,
        };
      });

    return markets;
  };

  // fetchCurrentLeverage = async () => {
  //   // 1. fetch all existing positions with v1 endpoint
  //   // this will tell us the current leverage for symbols where we had a position
  //   const {
  //     data: { data: allPositions },
  //   } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.POSITIONS, {
  //     params: {
  //       productType: this.apiProductType,
  //       marginCoin: this.apiMarginCoin,
  //     },
  //   });
  //
  //   const symbolLeverage = uniqBy(
  //     allPositions.map((d) => ({ symbol: d.symbol, leverage: d.leverage })),
  //     'symbol',
  //   );
  //
  //   symbolLeverage.forEach((d) => {
  //     this.leverageHash[d.symbol] = d.leverage;
  //   });
  //
  //   // use allPositions opportunity to set if we are in hedge mode
  //   if (allPositions.length > 0) {
  //     const isHedged = allPositions.some((p) => p.holdMode === 'double_hold');
  //     this.store.setSetting('isHedged', isHedged);
  //   }
  //
  //   // 2. fetch all the remaining markets on account endpoint
  //   // this will tell us the current leverage for symbols where we never had a position
  //   const remainingMarkets = this.store.markets.filter((m) => !symbolLeverage.find((d) => d.symbol === m.symbol));
  //
  //   for (const market of remainingMarkets) {
  //     if (!this.isDisposed) {
  //       try {
  //         const {
  //           data: { data },
  //         } = await this.xhr.get(ENDPOINTS.ACCOUNT, {
  //           params: {
  //             symbol: market.id,
  //             marginCoin: this.apiMarginCoin,
  //           },
  //         });
  //
  //         this.leverageHash[market.id] = data.crossMarginLeverage;
  //       } catch (err: any) {
  //         this.emitAndLogError(err?.response?.data?.msg || err?.message);
  //       }
  //     }
  //   }
  // };
  //
  // fetchLeverageBrackets = async () => {
  //   for (const market of this.store.markets) {
  //     if (!this.isDisposed) {
  //       try {
  //         const {
  //           data: { data },
  //         } = await this.xhr.get(ENDPOINTS.SYMBOL_LEVERAGE, {
  //           params: { symbol: market.id },
  //         });
  //         this.store.updateMarket(market, {
  //           limits: {
  //             ...market.limits,
  //             leverage: {
  //               min: parseInt(data.minLeverage, 10),
  //               max: parseInt(data.maxLeverage, 10),
  //             },
  //           },
  //         });
  //       } catch (err: any) {
  //         this.emitAndLogError(err?.response?.data?.msg || err?.message);
  //       }
  //     }
  //   }
  // };

  fetchTickers = async () => {
    const {
      data: { data },
    } = await this.xhr.get(ENDPOINTS.TICKERS, {
      params: { productType: this.apiProductType },
    });

    const tickers: Ticker[] = data.reduce((acc: Ticker[], t: Record<string, any>) => {
      const market = this.store.marketsById[t.symbol];

      if (!market) return acc;

      const last = parseFloat(t.lastPr);

      const ticker: Ticker = {
        id: market.id,
        symbol: market.symbol,
        bid: parseFloat(t.bidPr),
        ask: parseFloat(t.askPr),
        last,
        mark: last,
        index: parseFloat(t.indexPrice),
        percentage: parseFloat(t.changeUtc24h) * 100,
        fundingRate: parseFloat(t.fundingRate),
        fundingTime: 0, // not provided by the API, will be fetched with WS
        openInterest: 0, // not provided by the API, will be fetched with WS
        volume: parseFloat(t.baseVolume),
        quoteVolume: parseFloat(t.quoteVolume),
      };

      return [...acc, ticker];
    }, []);

    return tickers;
  };

  fetchOrders = async () => {
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this.fetchUtaOrders();
    }

    const normalOrders = await this.fetchNormalOrders();
    const algoOrders = await this.fetchAlgoOrders();
    const tslOrders = await this.fetchTrailingStopOrders();

    return [...normalOrders, ...algoOrders, ...tslOrders];
  };

  fetchNormalOrders = async () => {
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this.fetchUtaNormalOrders();
    }

    const allOrders: Array<Record<string, any>> = [];
    let endId: string | undefined;
    const maxIterations = 20; // Safety limit to prevent infinite loops
    let iterations = 0;

    while (iterations < maxIterations) {
      const params: Record<string, any> = {
        productType: this.apiProductType,
        limit: '200', // Maximum allowed by Bitget API
      };

      if (endId) {
        params.idLessThan = endId;
      }

      const {
        data: { data },
      } = await this.xhr.get<{ data: { entrustedList: Array<Record<string, any>>; endId: string } }>(ENDPOINTS.ORDERS, {
        params,
      });

      if (!Array.isArray(data?.entrustedList) || data.entrustedList.length === 0) {
        break;
      }

      allOrders.push(...data.entrustedList);

      // If endId is not provided or is the same, we've reached the end
      if (!data.endId || data.endId === endId) {
        break;
      }

      endId = data.endId;
      iterations++;
    }

    return allOrders.flatMap(this.mapOrder);
  };

  private fetchUtaOrders = async (): Promise<Order[]> => {
    const [normalOrders, strategyOrders] = await Promise.all([this.fetchUtaNormalOrders(), this.fetchUtaStrategyOrders()]);
    return [...normalOrders, ...strategyOrders];
  };

  private fetchUtaNormalOrders = async (): Promise<Order[]> => {
    const allOrders: BITGET_UTA_ORDER[] = [];
    let cursor: string | undefined;
    const maxIterations = 20;

    for (let iterations = 0; iterations < maxIterations; iterations += 1) {
      const params: Record<string, any> = {
        category: this.apiUtaCategory,
        limit: '100',
      };
      if (cursor) params.cursor = cursor;

      const { data } = await this.xhr.get<FETCH_UTA_ORDERS_RESPONSE>(ENDPOINTS.UTA_ORDERS, { params });

      if (!isBitgetSuccessCode(data.code)) {
        this.emitAndLogError(getBitgetApiErrorMessage(data, 'Bitget UTA open orders request failed'));
        return allOrders.flatMap(this.mapOrder);
      }

      const list = Array.isArray(data.data?.list) ? data.data.list : [];
      if (!list.length) break;
      allOrders.push(...list);

      const nextCursor = data.data?.cursor;
      if (!nextCursor || nextCursor === cursor) break;
      cursor = nextCursor;
    }

    return allOrders.flatMap(this.mapOrder);
  };

  private fetchUtaStrategyOrders = async (): Promise<Order[]> => {
    const { data } = await this.xhr.get<{
      code: string;
      msg: string;
      requestTime: number;
      data: BITGET_UTA_ORDER[] | { list?: BITGET_UTA_ORDER[] } | null;
    }>(ENDPOINTS.UTA_STRATEGY_ORDERS, {
      params: {
        category: this.apiUtaCategory,
        type: 'tpsl',
      },
    });

    if (!isBitgetSuccessCode(data.code)) {
      this.emitAndLogError(getBitgetApiErrorMessage(data, 'Bitget UTA strategy orders request failed'));
      return [];
    }

    const rows = Array.isArray(data.data) ? data.data : Array.isArray(data.data?.list) ? data.data.list : [];
    return rows.flatMap(this.mapOrder);
  };

  fetchAlgoOrders = async () => {
    // Bitget V2 orders-plan-pending with planType=profit_loss returns both
    // sized TP/SL (loss_plan/profit_plan) and position-level TP/SL (pos_loss/pos_profit).
    const allOrders: Array<Record<string, any>> = [];
    let endId: string | undefined;
    const maxIterations = 20;
    let iterations = 0;

    while (iterations < maxIterations) {
      const params: Record<string, any> = {
        productType: this.apiProductType,
        planType: 'profit_loss',
        limit: '200',
      };

      if (endId) {
        params.idLessThan = endId;
      }

      const {
        data: { data },
      } = await this.xhr.get<{ data: { entrustedList: Array<Record<string, any>>; endId?: string } }>(
        ENDPOINTS.ALGO_ORDERS,
        {
          params,
        },
      );

      if (!Array.isArray(data?.entrustedList) || data.entrustedList.length === 0) {
        break;
      }

      allOrders.push(...data.entrustedList);

      if (!data.endId || data.endId === endId) {
        break;
      }

      endId = data.endId;
      iterations++;
    }

    return allOrders.flatMap(this.mapOrder);
  };

  fetchTrailingStopOrders = async () => {
    const allOrders: Array<Record<string, any>> = [];
    let endId: string | undefined;
    const maxIterations = 20; // Safety limit to prevent infinite loops
    let iterations = 0;

    while (iterations < maxIterations) {
      const params: Record<string, any> = {
        productType: this.apiProductType,
        planType: 'track_plan',
        limit: '200', // Maximum allowed by Bitget API
      };

      if (endId) {
        params.idLessThan = endId;
      }

      const {
        data: { data },
      } = await this.xhr.get<{
        data: Array<Record<string, any>> | { entrustedList: Array<Record<string, any>>; endId?: string };
      }>(ENDPOINTS.ALGO_ORDERS, {
        params,
      });

      // Handle both response formats (array or object with entrustedList)
      let orders: Array<Record<string, any>> = [];
      if (Array.isArray(data)) {
        orders = data;
        // Array response doesn't support pagination, so we just return what we get
        return orders.flatMap(this.mapOrder);
      } else if (data?.entrustedList) {
        orders = data.entrustedList;

        if (orders.length === 0) {
          break;
        }

        allOrders.push(...orders);

        // If endId is not provided or is the same, we've reached the end
        if (!data.endId || data.endId === endId) {
          break;
        }

        endId = data.endId;
      } else {
        break;
      }

      iterations++;
    }

    return allOrders.flatMap(this.mapOrder);
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 500, 1000);
    const interval = INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);
    const intervalMs = (timeframeToSeconds[opts.interval] || 0) * 1000;

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start = opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);
    const startTime = start.valueOf();
    const endTime = opts.from
      ? Math.min(end.valueOf(), startTime + Math.min(BITGET_CANDLE_MAX_WINDOW_MS, intervalMs * limit))
      : end.valueOf();
    const shouldSendStartTime = Boolean(opts.from) && startTime < endTime;

    const {
      data: { data },
    } = await this.xhr.get(ENDPOINTS.KLINE, {
      params: {
        symbol: opts.symbol,
        granularity: interval,
        endTime,
        ...(shouldSendStartTime && { startTime }),
        productType: this.apiProductType,
        limit,
      },
    });

    if (!data) {
      return [];
    }

    const candles: Candle[] = data.map((c: string[]) => {
      return {
        timestamp: parseInt(c[0], 10) / 1000,
        open: parseFloat(c[1]),
        high: parseFloat(c[2]),
        low: parseFloat(c[3]),
        close: parseFloat(c[4]),
        volume: parseFloat(c[6]),
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

  cancelAllOrders = async () => {
    try {
      await this.ensureAccountTypeDetected();
      if (this.usesUtaRouting) {
        const [normalOrders, strategyOrders] = await Promise.all([this.fetchUtaNormalOrders(), this.fetchUtaStrategyOrders()]);
        await this.cancelUtaNormalOrders(normalOrders);
        await this.cancelUtaStrategyOrders(strategyOrders);
        return;
      }

      await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ALL_ORDERS, {
        productType: this.apiProductType,
        marginCoin: this.apiMarginCoin,
      });
      this.schedulePrivateStreamWriteCheck();
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  cancelOrders = async (orders: Order[]) => {
    const [virtualTPSLOrders, otherOrders] = partition<Order>(orders, this.isVirtualTPSLOrder);
    const [algoOrders, normalOrders] = partition<Order>(otherOrders, this.isAlgoOrder);
    const normalAndVirtualTPSL = [...normalOrders, ...virtualTPSLOrders];

    if (normalAndVirtualTPSL.length) await this.cancelNormalOrders(normalAndVirtualTPSL);
    if (algoOrders.length) await this.cancelAlgoOrders(algoOrders);
  };

  cancelNormalOrders = async (orders: Order[]) => {
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this.cancelUtaNormalOrders(orders);
    }

    // might include virtual TPSL
    orders = orders.map((o) => ({
      ...o,
      id: o.id.split('__')[0],
    }));
    orders = uniqBy(orders, 'id');

    const grouped = groupBy(orders, 'symbol');

    for (const [key, symbolOrders] of Object.entries(grouped)) {
      const symbol = `${key}`;
      const lots = chunk(symbolOrders, 50);
      for (const lot of lots) {
        const orderIdList = lot.map((o) => ({ orderId: o.id }));

        try {
          await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDERS, {
            symbol,
            orderIdList,
            marginCoin: this.apiMarginCoin,
            productType: this.apiProductType,
          });
        } catch (err: any) {
          this.emitAndLogError(err?.response?.data?.msg || err?.message);
          throw err;
        }
      }
    }
    if (orders.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
  };

  private cancelUtaNormalOrders = async (orders: Order[]) => {
    const normalizedOrders = uniqBy(
      orders.map((order) => ({
        ...order,
        id: order.id.split('__')[0],
      })),
      'id',
    );

    for (const order of normalizedOrders) {
      try {
        const orderId = order.info?.orderId || (order.info?.clientOid === order.id ? undefined : order.id);
        const clientOid = order.info?.clientOid || (!orderId ? order.id : undefined);

        await this.unlimitedXHR.post(
          ENDPOINTS.UTA_CANCEL_ORDER,
          omitUndefined({
            category: this.apiUtaCategory,
            orderId,
            clientOid,
          }),
        );
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
    if (normalizedOrders.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
  };

  private cancelUtaStrategyOrders = async (
    orders: Order[],
    opts: { readonly treatNotFoundAsSuccess?: boolean } = {},
  ) => {
    const treatNotFoundAsSuccess = opts.treatNotFoundAsSuccess ?? true;
    const normalizedOrders = uniqBy(orders, 'id');

    for (const order of normalizedOrders) {
      try {
        const orderId = order.info?.orderId || (order.info?.clientOid === order.id ? undefined : order.id);
        const clientOid = order.info?.clientOid || (!orderId ? order.id : undefined);

        const { data } = await this.unlimitedXHR.post(
          ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER,
          omitUndefined({
            orderId,
            clientOid,
          }),
        );

        if (!isBitgetSuccessCode(data?.code)) {
          const message = getBitgetApiErrorMessage(data, 'Bitget UTA cancel strategy order request failed');
          if (treatNotFoundAsSuccess && isBitgetUtaStrategyOrderNotFound(message)) {
            this.log(`Bitget UTA strategy order ${order.id} was already absent during cancel`, LogSeverity.Debug);
            continue;
          }
          throw new Error(message);
        }
      } catch (err: any) {
        const message = err?.response?.data?.msg || err?.message;
        if (treatNotFoundAsSuccess && isBitgetUtaStrategyOrderNotFound(message)) {
          this.log(`Bitget UTA strategy order ${order.id} was already absent during cancel`, LogSeverity.Debug);
          continue;
        }
        this.emitAndLogError(message);
        throw err;
      }
    }
    if (normalizedOrders.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
  };

  cancelAlgoOrders = async (orders: Order[]) => {
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this.cancelUtaStrategyOrders(orders);
    }

    for (const order of orders) {
      try {
        const planType = this.getCancelPlanType(order);
        await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ALGO_ORDER, {
          orderId: order.id,
          symbol: `${order.symbol}`,
          marginCoin: this.apiMarginCoin,
          productType: this.apiProductType,
          planType,
        });
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
    if (orders.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      const symbolOrders = this.store.orders.filter((o) => o.symbol === symbol);
      await this.cancelOrders(symbolOrders);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  // Resolve position-mode + margin-type for a symbol. Prefers an existing
  // position (authoritative), else falls back to fetchPositionMetadata, which
  // reads posMode/marginMode off the account endpoint WITHOUT needing an open
  // position. The fallback is load-bearing in hosted-exchange-server mode:
  // order writes run on the never-started main-thread mirror, whose
  // store.loaded.positions is false, so getOrFetchPositionsForSymbol returns []
  // for a flat symbol. The old code then threw before any order POST — and
  // because the throw was raw (not via emitAndLogError), it produced no toast
  // and no log, so orders silently never reached the exchange. bitget is the
  // only adapter that gates every order on this resolution (woo/bitmex don't).
  private async resolveSymbolPositioning(
    symbol: string,
  ): Promise<{ positionMode?: PositionMode; marginType?: MarginType }> {
    const positionsForSymbol = await this.getOrFetchPositionsForSymbol(symbol);
    const firstPosition = positionsForSymbol[0];
    if (firstPosition?.positionMode && firstPosition?.marginType) {
      return { positionMode: firstPosition.positionMode, marginType: firstPosition.marginType };
    }

    const cached = this._positioningCache[symbol];
    if (cached && Date.now() - cached.at < BitgetExchange.POSITIONING_CACHE_TTL_MS) {
      return cached.value;
    }
    const inflight = this._positioningInflight[symbol];
    if (inflight) return inflight;

    const promise = (async () => {
      const [metadata] = await this.fetchPositionMetadata([symbol]);
      const value = { positionMode: metadata?.positionMode, marginType: metadata?.marginType };
      if (value.positionMode && value.marginType) {
        this._positioningCache[symbol] = { at: Date.now(), value };
      }
      return value;
    })();
    this._positioningInflight[symbol] = promise;
    try {
      return await promise;
    } finally {
      delete this._positioningInflight[symbol];
    }
  }

  async getOrFetchPositionMode(symbol: string): Promise<PositionMode> {
    const { positionMode } = await this.resolveSymbolPositioning(symbol);
    if (!positionMode) {
      const msg = this.usesUtaRouting
        ? `Could not determine Bitget UTA position mode for symbol ${symbol}`
        : `Could not determine position mode for symbol ${symbol}`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }
    return positionMode;
  }

  async getOrFetchMarginType(symbol: string): Promise<MarginType> {
    const { marginType } = await this.resolveSymbolPositioning(symbol);
    if (!marginType) {
      const msg = this.usesUtaRouting
        ? `Could not determine Bitget UTA margin type for symbol ${symbol}`
        : `Could not determine margin type for symbol ${symbol}`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }
    return marginType;
  }

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      if (this.isAlgoOrder(opts)) {
        if (opts.type === OrderType.TrailingStopLoss) {
          const msg = 'Bitget UTA trailing-stop orders are not supported yet.';
          this.emitAndLogError(msg);
          throw new Error(msg);
        }
        return await this.placeUtaStrategyOrders([await this.formatUtaStrategyOrder(opts)]);
      }

      const payloads = await this.formatUtaCreateOrder(opts);
      return await this.placeUtaOrders(payloads);
    }

    if (this.isAlgoOrder(opts)) {
      const payload = await this.formatAlgoOrder(opts);
      return await this.placeAlgoOrders([payload]);
    }

    const payloads = await this.formatCreateOrder(opts);
    return await this.placeOrderBatch(payloads);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    await this.ensureAccountTypeDetected();
    const [algoOrders, normalOrders] = partition(orders, this.isAlgoOrder);

    if (this.usesUtaRouting) {
      const trailingStopOrder = algoOrders.find((order) => order.type === OrderType.TrailingStopLoss);
      if (trailingStopOrder) {
        const msg = 'Bitget UTA trailing-stop orders are not supported yet.';
        this.emitAndLogError(msg);
        throw new Error(msg);
      }

      const normalOrdersOptsWithEmpty = (
        await Promise.all(normalOrders.map((o) => this.formatUtaCreateOrder(o)))
      ).flat();
      const normalOrdersOpts = normalOrdersOptsWithEmpty.filter((o) => this.toFiniteNumber(o.qty) > 0);
      const strategyOrdersOpts = await Promise.all(algoOrders.map((o) => this.formatUtaStrategyOrder(o)));
      return [...(await this.placeUtaOrders(normalOrdersOpts)), ...(await this.placeUtaStrategyOrders(strategyOrdersOpts))];
    }

    const normalOrdersOptsWithEmpty = (await Promise.all(normalOrders.map((o) => this.formatCreateOrder(o)))).flat();
    const normalOrdersOpts = normalOrdersOptsWithEmpty.filter((o) => o.size > 0);

    // const derivedAlgoOrders = this.deriveAlgoOrdersFromNormalOrdersOpts(orders);
    const algoOrdersOpts = await Promise.all(algoOrders.map((o) => this.formatAlgoOrder(o)));

    return [...(await this.placeOrderBatch(normalOrdersOpts)), ...(await this.placeAlgoOrders(algoOrdersOpts))];
  };

  formatCreateOrder = async (opts: PlaceOrderOpts) => {
    await this.ensureAccountTypeDetected();
    if (this.usesUtaRouting) {
      return this.formatUtaCreateOrder(opts);
    }

    // there are some hacks in here like TIF parsing, etc to support v2 quickly

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }
    const positionMode = await this.getOrFetchPositionMode(opts.symbol);

    const [side, tradeSide] = this.getOrderSideV2(opts, positionMode);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;

    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    const amount = adjust(opts.amount, pAmount);

    const req = omitUndefined({
      symbol: market.symbol,
      size: amount ? `${amount}` : undefined,
      price: price ? `${price}` : undefined,
      side,
      tradeSide,
      orderType: inverseObj(ORDER_TYPE)[opts.type],
      ...(opts.type !== OrderType.Market ? { force: timeInForce === 'normal' ? 'gtc' : timeInForce } : {}),
      reduceOnly: positionMode === PositionMode.Hedged ? undefined : opts.reduceOnly ? 'YES' : 'NO',
      presetStopSurplusPrice: opts.takeProfit ? `${adjust(opts.takeProfit, pPrice)}` : undefined,
      presetStopLossPrice: opts.stopLoss ? `${adjust(opts.stopLoss, pPrice)}` : undefined,
    });

    // Only split into lots when maxSize is a valid positive size. Guard against
    // maxSize <= 0: a zero max (seen on the hosted-server mirror, whose streamed
    // market can carry limits.amount.max === 0) made lots = ceil(amount / 0) =
    // Infinity, and lodash `times(Infinity)` returns [] — so formatCreateOrder
    // produced ZERO payloads and the order silently never POSTed.
    const shouldSplit = maxSize > 0 && amount > maxSize;
    const lots = shouldSplit ? Math.ceil(amount / maxSize) : 1;
    const rest = shouldSplit ? adjust(amount % maxSize, pAmount) : 0;
    const lotSize = adjust((amount - rest) / lots, pAmount);

    const payloads: Array<Record<string, any>> = times(lots, () => ({
      ...req,
      size: `${lotSize}`,
    }));

    if (rest) {
      // bitget's order field is `size`, not `quantity` — the remainder lot must
      // override `size` (req already carries the full `amount` as `size`), else
      // the remainder would re-place the full amount.
      payloads.push({ ...req, size: `${rest}` });
    }

    const clientOrderId = this.resolveCallerClientOrderId(opts);
    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
    for (const payload of payloads) {
      payload.clientOid = clientOrderId ?? uuid();
    }

    return payloads;
  };

  private formatUtaCreateOrder = async (opts: PlaceOrderOpts): Promise<Array<Record<string, any>>> => {
    if (opts.type !== OrderType.Limit && opts.type !== OrderType.Market) {
      const msg = 'Bitget UTA currently supports only normal market and limit orders.';
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    const positionMode = await this.getOrFetchPositionMode(opts.symbol);
    const marginType = await this.getOrFetchMarginType(opts.symbol);
    const { side, posSide } = this.getOrderSideUta(opts, positionMode);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;
    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];
    const amount = adjust(opts.amount, pAmount);

    const req = omitUndefined({
      category: this.apiUtaCategory,
      symbol: market.id,
      qty: amount ? `${amount}` : undefined,
      price: price ? `${price}` : undefined,
      side,
      posSide,
      orderType: inverseObj(ORDER_TYPE)[opts.type],
      timeInForce: opts.type !== OrderType.Market ? (timeInForce === 'normal' ? 'gtc' : timeInForce) : undefined,
      reduceOnly: positionMode === PositionMode.Hedged ? undefined : opts.reduceOnly ? 'yes' : 'no',
      marginMode: marginType === MarginType.Cross ? 'crossed' : 'isolated',
      takeProfit: opts.takeProfit ? `${adjust(opts.takeProfit, pPrice)}` : undefined,
      stopLoss: opts.stopLoss ? `${adjust(opts.stopLoss, pPrice)}` : undefined,
      tpTriggerBy: opts.takeProfit ? this.getUtaStrategyTriggerBy(opts.tpTriggerType) : undefined,
      slTriggerBy: opts.stopLoss ? this.getUtaStrategyTriggerBy(opts.slTriggerType) : undefined,
      tpOrderType: opts.takeProfit ? 'market' : undefined,
      slOrderType: opts.stopLoss ? 'market' : undefined,
    });

    const shouldSplit = maxSize > 0 && amount > maxSize;
    const lots = shouldSplit ? Math.ceil(amount / maxSize) : 1;
    const rest = shouldSplit ? adjust(amount % maxSize, pAmount) : 0;
    const lotSize = adjust((amount - rest) / lots, pAmount);

    const payloads: Array<Record<string, any>> = times(lots, () => ({
      ...req,
      qty: `${lotSize}`,
    }));

    if (rest) {
      payloads.push({ ...req, qty: `${rest}` });
    }

    const clientOrderId = this.resolveCallerClientOrderId(opts);
    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
    for (const payload of payloads) {
      payload.clientOid = clientOrderId ?? uuid();
    }

    return payloads;
  };

  private formatUtaStrategyOrder = async (opts: PlaceOrderOpts): Promise<Record<string, any>> => {
    if (opts.type !== OrderType.StopLoss && opts.type !== OrderType.TakeProfit) {
      const msg = 'Bitget UTA currently supports only standalone stop-loss and take-profit strategy orders.';
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    if (!opts.price) {
      const msg = `Bitget UTA ${opts.type} orders require a trigger price.`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    const positionMode = await this.getOrFetchPositionMode(opts.symbol);
    const { side, posSide } = this.getUtaStrategyOrderSide(opts, positionMode);
    const [, rawAmount] = await this.getOrderPlanTypeAndAmount(opts);
    let qty = rawAmount > 0 ? adjust(rawAmount, market.precision.amount) : 0;

    // Infinity-stops marker from the UI (amount = 0) means "close the whole position".
    // Bitget's docs describe `tpslMode: 'full'` (no qty) for that, but CCXT and every
    // other Bitget adapter always send `partial` with an explicit qty from the live
    // position. Match the CCXT-proven shape whenever we have a size to send; fall
    // back to the documented `full` shape only if there is no live position to
    // reference (pre-position TP/SL). This avoids relying on Bitget behavior that
    // no live adapter has proven for us.
    if (qty === 0) {
      const positions = await this.getOrFetchPositionsForSymbol(opts.symbol);
      const targetPositionSide = opts.side === OrderSide.Sell ? PositionSide.Long : PositionSide.Short;
      const targetPosition =
        positionMode === PositionMode.Hedged
          ? positions.find((p) => p.side === targetPositionSide && p.contracts > 0)
          : positions.find((p) => p.contracts > 0);
      if (targetPosition && targetPosition.contracts > 0) {
        qty = adjust(targetPosition.contracts, market.precision.amount);
      }
    }

    const triggerPrice = `${adjust(opts.price, market.precision.price)}`;

    return omitUndefined({
      category: this.apiUtaCategory,
      symbol: market.id,
      type: 'tpsl',
      side,
      posSide,
      tpslMode: qty > 0 ? 'partial' : 'full',
      qty: qty > 0 ? `${qty}` : undefined,
      reduceOnly: 'yes',
      clientOid: uuid(),
      takeProfit: opts.type === OrderType.TakeProfit ? triggerPrice : undefined,
      stopLoss: opts.type === OrderType.StopLoss ? triggerPrice : undefined,
      tpTriggerBy: opts.type === OrderType.TakeProfit ? this.getUtaStrategyTriggerBy(opts.tpTriggerType) : undefined,
      slTriggerBy: opts.type === OrderType.StopLoss ? this.getUtaStrategyTriggerBy(opts.slTriggerType) : undefined,
      tpOrderType: opts.type === OrderType.TakeProfit ? 'market' : undefined,
      slOrderType: opts.type === OrderType.StopLoss ? 'market' : undefined,
    });
  };

  formatAlgoOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    const payload: Record<string, any> = {
      marginCoin: this.apiMarginCoin,
      symbol: market.id,
      clientOid: uuid(),
      productType: this.apiProductType,
    };

    const positionMode = await this.getOrFetchPositionMode(opts.symbol);

    // If user is in oneWay mode, always set reduceOnly to "YES"
    if (positionMode === PositionMode.OneWay) {
      payload.reduceOnly = 'YES';
    }

    if (opts.type === OrderType.TakeProfit && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      payload.triggerPrice = `${price}`;
      payload.holdSide = this.getAlgoOrderSide(opts, positionMode);
      const [planType, amount] = await this.getOrderPlanTypeAndAmount(opts);
      payload.planType = planType;
      if (amount) payload.size = `${adjust(amount, market.precision.amount)}`;
      // Bitget V2 API: mark_price (default) or fill_price (last price)
      payload.triggerType = opts.tpTriggerType === OrderStopTriggerType.LastPrice ? 'fill_price' : 'mark_price';
    }

    if (opts.type === OrderType.StopLoss && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      payload.triggerPrice = `${price}`;
      payload.holdSide = this.getAlgoOrderSide(opts, positionMode);
      const [planType, amount] = await this.getOrderPlanTypeAndAmount(opts);
      payload.planType = planType;
      if (amount) payload.size = `${adjust(amount, market.precision.amount)}`;
      // Bitget V2 API: mark_price (default) or fill_price (last price)
      payload.triggerType = opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'fill_price' : 'mark_price';
    }

    if (opts.type === OrderType.TrailingStopLoss && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      const amount = adjust(opts.amount, market.precision.amount);

      const ticker = this.store.tickersBySymbol[opts.symbol];
      if (!ticker) throw new Error(`Ticker ${opts.symbol} not found`);

      const priceDistance = adjust(
        Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
        market.precision.price,
      );

      const distancePercentage = Math.round(((priceDistance * 100) / ticker.last) * 10) / 10;

      payload.triggerPrice = `${price}`;
      payload.size = `${amount}`;
      payload.planType = 'track_plan';
      const marginType = await this.getOrFetchMarginType(opts.symbol);
      payload.marginMode = marginType === MarginType.Cross ? 'crossed' : 'isolated';
      payload.orderType = 'market';
      payload.triggerType = opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'fill_price' : 'mark_price';
      payload.side = opts.side === OrderSide.Buy ? 'buy' : 'sell';
      if (positionMode === PositionMode.Hedged) payload.tradeSide = 'close';
      payload.callbackRatio = `${distancePercentage}`;
    }

    return payload;
  };

  placeOrderBatch = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];
    const grouped = groupBy(payloads, 'symbol');

    for (const [symbol, orders] of Object.entries(grouped)) {
      const marginType = await this.getOrFetchMarginType(symbol);
      const lots = chunk(orders, 50);
      for (const lot of lots) {
        try {
          const {
            data: { data },
          } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, {
            symbol,
            productType: this.apiProductType,
            marginCoin: this.apiMarginCoin,
            marginMode: marginType,
            orderList: lot.map((o) => omit(o, 'symbol')),
          });

          const oIds = data?.successList?.map?.((obj: any) => obj.orderId);
          if (oIds) newOrderIds.push(...oIds);

          data?.failureList?.forEach?.((obj: any) => {
            this.emitAndLogError(obj.errorMsg);
            throw new Error(obj.errorMsg);
          });
        } catch (err: any) {
          this.emitAndLogError(err?.response?.data?.msg || err?.message);
          throw err;
        }
      }
    }

    if (newOrderIds.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
    return newOrderIds;
  };

  private placeUtaOrders = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];

    for (const payload of payloads) {
      try {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.UTA_PLACE_ORDER, payload);

        if (!isBitgetSuccessCode(data?.code)) {
          throw new Error(data?.msg || 'Bitget UTA place order request failed');
        }

        const orderId = data?.data?.orderId || data?.data?.clientOid || payload.clientOid;
        if (orderId) newOrderIds.push(orderId);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    if (newOrderIds.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
    return newOrderIds;
  };

  private placeUtaStrategyOrders = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];

    for (const payload of payloads) {
      try {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.UTA_PLACE_STRATEGY_ORDER, payload);

        if (!isBitgetSuccessCode(data?.code)) {
          throw new Error(data?.msg || 'Bitget UTA place strategy order request failed');
        }

        const orderId = data?.data?.orderId || data?.data?.clientOid || payload.clientOid;
        if (orderId) newOrderIds.push(orderId);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    if (newOrderIds.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
    return newOrderIds;
  };

  placeAlgoOrders = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];

    for (const payload of payloads) {
      try {
        const endpoint =
          payload.planType === 'track_plan' ? ENDPOINTS.PLACE_TRAILING_STOP_ORDER : ENDPOINTS.PLACE_ALGO_ORDER;

        const { data } = await this.unlimitedXHR.post(endpoint, payload);

        if (parseInt(data.code || '0') !== 0) {
          throw new Error(data.msg);
        }

        if (data?.data?.orderId) {
          newOrderIds.push(data.data.orderId);
        }
      } catch (err: any) {
        this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    if (newOrderIds.length > 0) {
      this.schedulePrivateStreamWriteCheck();
    }
    return newOrderIds;
  };

  updateVirtualTpSlOrder = async ({ order, update }: UpdateOrderOpts, relatedOrders: Order[]) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    const parentOrder = relatedOrders.find((o) => !o.id.includes('__'));
    if (!parentOrder) {
      this.emitAndLogError(`Parent order not found for ${order.id}`);
      throw new Error(`Parent order not found for ${order.id}`);
    }

    const stopLossOrder = relatedOrders.find((o) => o.id.endsWith('__stop_loss'));
    const takeProfitOrder = relatedOrders.find((o) => o.id.endsWith('__take_profit'));

    const newFormattedPayloads = await this.formatCreateOrder({ ...parentOrder });

    const payload: Record<string, any> = omitUndefined({
      ...newFormattedPayloads[0],
      price: `${parentOrder.price}`,
      size: `${parentOrder.amount}`,
      presetStopSurplusPrice: takeProfitOrder ? `${takeProfitOrder.price}` : undefined,
      presetStopLossPrice: stopLossOrder ? `${stopLossOrder.price}` : undefined,
    });
    if ('amount' in update) {
      const amount = adjust(update.amount, market.precision.amount);
      payload.size = `${amount}`;
    }
    // if amending parent
    if (order.id === parentOrder.id) {
      if ('price' in update) {
        const price = adjust(update.price, market.precision.price);
        payload.price = `${price}`;
      }
    } else if (order.id.endsWith('__stop_loss')) {
      if ('price' in update) {
        const price = adjust(update.price, market.precision.price);
        payload.presetStopLossPrice = `${price}`;
      }
    } else if (order.id.endsWith('__take_profit')) {
      if ('price' in update) {
        const price = adjust(update.price, market.precision.price);
        payload.presetStopSurplusPrice = `${price}`;
      }
    }
    await this.cancelOrders([parentOrder]);
    return await this.placeOrderBatch([payload]);
  };

  private tryAmendLimitOrder = async (order: Order, update: UpdateOrderOpts['update']): Promise<string[] | null> => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) return null;

    // Keep parent+virtual TP/SL flow on existing cancel+replace path.
    const hasVirtualChildren = this.store.orders.some((o) => o.id.startsWith(`${order.id}__`));
    if (hasVirtualChildren) return null;

    const payload: Record<string, any> = {
      orderId: order.id,
      symbol: order.symbol,
      productType: this.apiProductType,
      newClientOid: uuid(),
    };

    // Bitget modify-order expects price and size together.
    // For single-field updates (drag price only / size only), pass the unchanged side explicitly.
    const nextPriceRaw = 'price' in update ? update.price : order.price;
    const nextSizeRaw = 'amount' in update ? update.amount : order.amount;

    if (!(nextPriceRaw > 0) || !(nextSizeRaw > 0)) {
      return [];
    }

    payload.newPrice = `${adjust(nextPriceRaw, market.precision.price)}`;
    payload.newSize = `${adjust(nextSizeRaw, market.precision.amount)}`;

    try {
      const { data } = await this.unlimitedXHR.post(ENDPOINTS.UPDATE_ORDER, payload);
      if (parseInt(data?.code || '0') !== 0) {
        return null;
      }
      this.schedulePrivateStreamWriteCheck();
      return [data?.data?.orderId || order.id];
    } catch {
      return null;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    // Check for virtual TPSL orders (preset TP/SL on limit orders) first
    if (this.isVirtualTPSLOrder(order)) {
      const parentId = order.id.split('__')[0];
      const relatedOrders = this.store.orders.filter((o) => o.id.split('__')[0] === parentId);
      if (relatedOrders.length > 1) {
        return this.updateVirtualTpSlOrder({ order, update }, relatedOrders);
      }
    }

    // Standalone algo orders (not attached to a limit order)
    if (this.isAlgoOrder(order)) {
      await this.ensureAccountTypeDetected();
      if (this.usesUtaRouting) {
        return this.updateUtaStrategyOrder({ order, update });
      }
      return this.updateAlgoOrder({ order, update });
    }

    await this.ensureAccountTypeDetected();
    const shouldUseLegacyBitgetDragUpdate = Boolean(order.info?.__tealstreetUseLegacyBitgetDragUpdate);
    if (order.type === OrderType.Limit && !shouldUseLegacyBitgetDragUpdate && !this.usesUtaRouting) {
      const amendedOrderIds = await this.tryAmendLimitOrder(order, update);
      if (amendedOrderIds !== null) {
        return amendedOrderIds;
      }
    }

    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
      timeInForce: order.timeInForce,
    };

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    await this.cancelOrders([order]);
    return await this.placeOrder(newOrder);
  };

  private updateUtaStrategyOrder = async ({ order, update }: UpdateOrderOpts) => {
    if (order.type === OrderType.TrailingStopLoss) {
      const msg = 'Bitget UTA trailing-stop orders are not supported yet.';
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    // Snapshot the original as a placeable payload BEFORE cancel, so a
    // rollback on place-failure can restore the stop the user thought they
    // had. UTA has no in-place amend, and a naive cancel+place pattern will
    // silently strand the user without a stop if the replacement fails
    // (rate limit, min-notional, size guard, position closed between calls).
    const originalPayload = await this.formatUtaStrategyOrder(this.buildUtaStrategyPlaceOptsFromOrder(order));

    const replacementOpts = this.buildUtaStrategyPlaceOptsFromOrder(order);
    if ('price' in update) replacementOpts.price = update.price;
    if ('amount' in update) replacementOpts.amount = update.amount;
    const replacementPayload = await this.formatUtaStrategyOrder(replacementOpts);

    await this.cancelUtaStrategyOrders([order], { treatNotFoundAsSuccess: false });
    try {
      return await this.placeUtaStrategyOrders([replacementPayload]);
    } catch (placeErr: any) {
      // Cancel already succeeded — the user has no live stop right now. Try
      // to restore the original as a best-effort rollback. Whether or not the
      // rollback succeeds we surface the original place failure so the caller
      // knows the requested update did not land.
      try {
        await this.placeUtaStrategyOrders([originalPayload]);
        this.emitAndLogError(
          `Bitget UTA update failed: ${placeErr?.message || 'place failed'}. Original stop was restored; please retry the update.`,
        );
      } catch (rollbackErr: any) {
        this.emitAndLogError(
          `Bitget UTA update failed and rollback also failed: ${placeErr?.message || 'place failed'} / ${rollbackErr?.message || 'rollback failed'}. Your stop is not on the book — please re-add it.`,
        );
      }
      throw placeErr;
    }
  };

  private buildUtaStrategyPlaceOptsFromOrder = (order: Order): Writable<PlaceOrderOpts> => {
    const opts: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
      timeInForce: order.timeInForce,
    };
    if ('tpTriggerType' in order) opts.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) opts.slTriggerType = order.slTriggerType;
    return opts;
  };

  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    // Strip virtual TPSL suffix (e.g., "123__stop_loss" -> "123") to get actual order ID
    const orderId = order.id.split('__')[0];

    const payload: Record<string, any> = {
      orderId,
      marginCoin: this.apiMarginCoin,
      symbol: market.id,
      productType: this.apiProductType,
      //planType: order?.info?.planType || 'pos_loss',
    };

    if ('price' in update) {
      const price = adjust(update.price, market.precision.price);
      payload.triggerPrice = `${price}`;
    }

    if ('amount' in update && Boolean(update.amount) && +update.amount > 0) {
      const amount = adjust(update.amount, market.precision.amount);
      payload.size = `${amount}`;
    } else if (order.amount && order.amount > 0) {
      // Preserve the order's current size to avoid converting sized stops to position-level
      const amount = adjust(order.amount, market.precision.amount);
      payload.size = `${amount}`;
    } else {
      // Position-level orders (amount=0) require empty size
      payload.size = '';
    }

    try {
      const {
        data: { data },
      } = await this.unlimitedXHR.post(ENDPOINTS.UPDATE_ALGO_ORDER, payload);
      if (data?.orderId) {
        this.schedulePrivateStreamWriteCheck();
      }
      return data?.orderId ? [data.orderId] : [];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number, side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    const position =
      (side ? this.store.positions.find((p) => p.symbol === symbol && p.side === side) : undefined) ??
      this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        // Per-side ISO leverage in hedge mode is the only case where the long/short
        // distinction matters. The v2 endpoint silently ignores `holdSide` on UTA
        // accounts (couples both sides); v3 honors per-side via `posSide`. Route to
        // v3 only when both conditions hold; v2 stays the default for cross / one-way
        // / classic accounts.
        await this.ensureAccountTypeDetected();
        const isPerSide = position.positionMode === PositionMode.Hedged && position.marginType === MarginType.Isolated;
        const useV3 = this.usesUtaRouting && isPerSide;

        const { data } = useV3
          ? await this.xhr.post(ENDPOINTS.SET_LEVERAGE_V3, {
              symbol: market.id,
              coin: this.apiMarginCoin,
              category: this.apiProductType.toUpperCase(),
              leverage: String(leverage),
              posSide: side === PositionSide.Short ? 'short' : 'long',
            })
          : await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
              symbol: market.id,
              marginCoin: this.apiMarginCoin,
              productType: this.apiProductType,
              leverage,
              ...(isPerSide ? { holdSide: side === PositionSide.Short ? 'short' : 'long' } : {}),
            });

        if (parseInt(data.code || '0') !== 0) {
          throw new Error(data.msg);
        }

        const positions = await this.fetchPositions([symbol]);
        this.store.addOrUpdatePositions(positions);

        // this.leverageHash[market.id] = leverage;
        // this.store.updatePositions([
        //   [{ id: createPositionId(symbol, PositionSide.Long) }, { leverage }],
        //   [{ id: createPositionId(symbol, PositionSide.Short) }, { leverage }],
        // ]);
      } catch (err: any) {
        this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }
    try {
      await this.ensureAccountTypeDetected();
      if (this.usesUtaRouting) {
        const market = this.store.marketsBySymbol[symbol];
        if (!market) throw new Error(`Market ${symbol} not found`);

        const [metadata] = await this.fetchUtaPositionMetadata([symbol]);
        const leverage = metadata?.leverage || market.limits.leverage.min;
        const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE_V3, {
          category: this.apiUtaCategory,
          symbol: market.id,
          leverage: String(leverage),
          marginMode: isolated ? 'isolated' : 'crossed',
        });

        if (!isBitgetSuccessCode(data.code)) {
          throw new Error(data.msg);
        }

        delete this._positioningCache[symbol];
        delete this._positioningInflight[symbol];
        const positions = await this.fetchPositions([symbol]);
        this.store.addOrUpdatePositions(positions);
        return;
      }

      const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_MODE, {
        productType: this.apiProductType,
        marginCoin: this.apiMarginCoin,
        marginMode: isolated ? 'isolated' : 'crossed',
        symbol,
      });
      if (parseInt(data.code || '0') !== 0) {
        throw new Error(data.msg);
      }
      // Margin type is per-symbol — drop only this symbol's cached positioning
      // so the next order re-resolves the new marginMode (a flat account has no
      // store row to override the cache otherwise).
      delete this._positioningCache[symbol];
      delete this._positioningInflight[symbol];
      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  changePositionMode = async (hedged: boolean) => {
    try {
      await this.ensureAccountTypeDetected();
      if (this.usesUtaRouting) {
        const { data } = await this.xhr.post(ENDPOINTS.UTA_SET_HOLD_MODE, {
          holdMode: hedged ? 'hedge_mode' : 'one_way_mode',
        });

        if (!isBitgetSuccessCode(data.code)) {
          throw new Error(data.msg);
        }

        this._positioningCache = {};
        this._positioningInflight = {};
        const positions = await this.fetchPositions();
        this.store.update({ positions });
        return;
      }

      const { data } = await this.xhr.post(ENDPOINTS.SET_POSITION_MODE, {
        productType: this.apiProductType,
        posMode: hedged ? 'hedge_mode' : 'one_way_mode',
      });

      if (parseInt(data.code || '0') !== 0) {
        throw new Error(data.msg);
      }
      // Position mode is global (positionModeIsGlobal) — flush the whole
      // positioning cache so the next order re-resolves the new posMode. On a
      // flat account there's no store position row to override stale values.
      this._positioningCache = {};
      this._positioningInflight = {};
      const positions = await this.fetchPositions();
      this.store.update({ positions });
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  // "info": {
  //   "symbol": "AAVEUSDT_UMCBL",
  //   "size": 1.2,
  //   "orderId": "1173546359219429379",
  //   "clientOid": "ZXjNIAJ2fTpei6R2VSrX",
  //   "filledQty": 0,
  //   "fee": 0,
  //   "price": 79.629,
  //   "state": "new",
  //   "side": "buy_single",
  //   "timeInForce": "normal",
  //   "totalProfits": 0,
  //   "posSide": "long",
  //   "marginCoin": "USDT",
  //   "presetStopLossPrice": 76.978,
  //   "filledAmount": 0,
  //   "orderType": "limit",
  //   "leverage": "10",
  //   "marginMode": "crossed",
  //   "reduceOnly": false,
  //   "enterPointSource": "API",
  //   "tradeSide": "buy_single",
  //   "holdMode": "single_hold",
  //   "orderSource": "normal",
  //   "cTime": "1715521176158",
  //   "uTime": "1715521176266"
  // }

  mapOrder = (o: Record<string, any>): Order[] => {
    const rawAmount = o.size || o.sz || o.qty || o.amount || '0';
    const rawFilled = o.baseVolume || o.filledQty || o.accFillSz || o.cumExecQty || '0';
    const symbol = (o.symbol || o.instId || '').replace(`_${this.apiProductType.toUpperCase()}`, '');
    const order: Writable<Order> = {
      id: o.orderId || o.clientOid || o.ordId || o.id,
      status: ORDER_STATUS[o.status || o.state || o.planStatus || o.orderStatus],
      symbol,
      type: ORDER_TYPE[o.orderType || o.ordType || o.planType],
      side: ORDER_SIDE[o.tradeSide || o.tS || o.side] || o.side,
      price: parseFloat(o.price || o.px || '0'),
      amount: parseFloat(rawAmount),
      filled: parseFloat(rawFilled),
      lastUpdate: virtualClock.getCurrentTimeMs(),
      reduceOnly: ['YES', 'yes', 'true'].includes(`${o.reduceOnly}`),
      remaining: subtract(parseFloat(rawAmount), parseFloat(rawFilled)),
      timeInForce: TIME_IN_FORCE[o.force || o.timeInForce || o.tiF] || OrderTimeInForce.GoodTillCancel,
      info: o,
    };

    if (o.planType) {
      order.type = ORDER_TYPE[o.planType];
      order.price = parseFloat(o.triggerPrice) || parseFloat(o.triggerPx);
      order.reduceOnly = true;

      // Parse trigger type from Bitget response: mark_price or fill_price (last price)
      const triggerType =
        o.triggerType === 'fill_price' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice;
      if (order.type === OrderType.TakeProfit) {
        order.tpTriggerType = triggerType;
      } else if (order.type === OrderType.StopLoss) {
        order.slTriggerType = triggerType;
      }

      if (order.info?.posMode === 'hedge_mode') {
        order.side = o.side === 'buy' ? OrderSide.Sell : OrderSide.Buy;
      }
    } else if (!order.type && (this.toFiniteNumber(o.stopLoss) > 0 || this.toFiniteNumber(o.takeProfit) > 0)) {
      const strategySide =
        ORDER_SIDE[o.side] || (o.posSide === 'short' ? OrderSide.Buy : o.posSide === 'long' ? OrderSide.Sell : order.side);
      const strategyOrders: Writable<Order>[] = [];

      if (this.toFiniteNumber(o.stopLoss) > 0) {
        strategyOrders.push({
          ...order,
          side: strategySide,
          type: OrderType.StopLoss,
          price: this.toFiniteNumber(o.stopLoss),
          reduceOnly: true,
          slTriggerType: o.slTriggerBy === 'market' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice,
        });
      }
      if (this.toFiniteNumber(o.takeProfit) > 0) {
        strategyOrders.push({
          ...order,
          side: strategySide,
          type: OrderType.TakeProfit,
          price: this.toFiniteNumber(o.takeProfit),
          reduceOnly: true,
          tpTriggerType: o.tpTriggerBy === 'market' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice,
        });
      }

      return strategyOrders;
    } else if (order.info?.reduceOnly === 'YES' && order.info?.posMode === 'hedge_mode') {
      order.side = o.side === 'buy' ? OrderSide.Sell : OrderSide.Buy;
    }

    let orders = [order];
    if (order.type === OrderType.Limit) {
      if (o.presetStopLossPrice || o.stopLossTriggerPrice || o.stopLoss) {
        orders.push({
          ...order,
          id: `${order.id}__stop_loss`,
          side: order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.StopLoss,
          price: parseFloat(o.presetStopLossPrice || o.stopLossTriggerPrice || o.stopLoss),
        });
      }
      if (o.presetStopSurplusPrice || o.stopSurplusTriggerPrice || o.presetTakeProfitPrice || o.takeProfit) {
        orders.push({
          ...order,
          id: `${order.id}__take_profit`,
          side: order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.TakeProfit,
          price: parseFloat(
            o.presetStopSurplusPrice || o.stopSurplusTriggerPrice || o.presetTakeProfitPrice || o.takeProfit,
          ),
        });
      }
    }
    return orders;
  };

  mapUtaPosition = (p: BITGET_UTA_POSITION): Position => {
    const symbol = this.normalizeBitgetSymbol(p.symbol);
    const positionMode = this.normalizeBitgetPositionMode(p);
    const side = this.normalizeBitgetPositionSide(p);
    const marginMode = this.normalizeBitgetMarginType(p.marginMode);
    const contracts = this.firstFiniteNumber([p.total, p.size]);
    const markPrice = this.firstFiniteNumber([p.markPrice, (p as Record<string, any>).marketPrice]);
    const entryPrice = this.firstFiniteNumber([p.avgPrice, p.openPriceAvg, (p as Record<string, any>).openAvgPrice, (p as Record<string, any>).avgPx]);
    const positionId = createPositionId(symbol, side, positionMode);
    const marginSize = this.firstFiniteNumber([p.positionBalance, p.marginSize], NaN);

    return {
      id: positionId,
      symbol,
      side,
      entryPrice,
      notional: multiply(contracts, markPrice || entryPrice),
      leverage: this.firstFiniteNumber([p.leverage, (p as Record<string, any>).lever], 1),
      unrealizedPnl: this.firstFiniteNumber([p.unrealisedPnl, p.unrealizedPL, (p as Record<string, any>).upl], 0),
      realizedPnl:
        this.toFiniteNumber(p.curRealisedPnl) +
        this.toFiniteNumber(p.totalFunding) +
        this.toFiniteNumber(p.openFeeTotal) +
        this.toFiniteNumber(p.closeFeeTotal) -
        this.toFiniteNumber(p.deductedFee),
      contracts,
      liquidationPrice: this.resolveLiquidationPrice(positionId, p.liquidationPrice ?? p.liqPrice ?? (p as Record<string, any>).liqPx),
      marginSize: marginSize > 0 ? marginSize : undefined,
      positionMode,
      marginType: marginMode,
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };
  };

  mapPosition = (p: Record<string, any>): Position => {
    // Guard against undefined/invalid payloads
    if (!p || typeof p !== 'object') {
      const fallbackSymbol = '';
      const side = PositionSide.Long;
      const positionMode = PositionMode.OneWay;
      return {
        id: createPositionId(fallbackSymbol, side, positionMode),
        symbol: fallbackSymbol,
        side,
        entryPrice: 0,
        notional: 0,
        leverage: 1,
        unrealizedPnl: 0,
        realizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        positionMode,
        marginType: MarginType.Cross,
        info: p,
        lastUpdate: virtualClock.getCurrentTimeMs(),
      };
    }

    const contracts = this.firstFiniteNumber([p.total, p.size, p.pos, p.openTotalPos, p.available]);
    const entryPrice = this.firstFiniteNumber([p.openPriceAvg, p.openAvgPrice, p.avgPrice, p.avgPx, p.averageOpenPrice]);
    const markPrice = this.firstFiniteNumber([p.markPrice, p.marketPrice], entryPrice);
    const symbol = this.normalizeBitgetSymbol(p.instId ?? p.symbol);
    const positionMode = this.normalizeBitgetPositionMode(p);
    const marginMode = this.normalizeBitgetMarginType(p.marginMode);
    const side = this.normalizeBitgetPositionSide(p);

    const marginSize = this.firstFiniteNumber([p.marginSize, p.margin, p.positionBalance], NaN);
    const positionId = createPositionId(symbol, side, positionMode);
    // Bitget V2 non-UTA position payloads (REST `POSITIONS_V2` and the
    // corresponding private WS `positions` topic) report realized PnL as
    // `achievedProfits + totalFee - deductedFee`. The UTA endpoint uses
    // `curRealisedPnl + totalFunding + openFee/closeFeeTotal - deductedFee`
    // via `mapUtaPosition`. Do NOT sum both here — for a UTA payload it
    // double-counts, and for a non-UTA payload the UTA fields are absent
    // (which we relied on earlier, but is fragile). Keep the branches split.
    const position: Position = {
      id: positionId,
      symbol,
      side,
      entryPrice,
      notional: multiply(contracts, markPrice || entryPrice),
      leverage: this.firstFiniteNumber([p.leverage, p.lever], 1),
      unrealizedPnl: this.firstFiniteNumber([p.unrealizedPL, p.unrealisedPnl, p.upl], 0),
      realizedPnl:
        this.toFiniteNumber(p.achievedProfits) +
        this.toFiniteNumber(p.totalFee) -
        this.toFiniteNumber(p.deductedFee),
      contracts,
      liquidationPrice: this.resolveLiquidationPrice(positionId, p.liquidationPrice ?? p.liqPrice ?? p.liqPx),
      marginSize: marginSize && marginSize > 0 ? marginSize : undefined,
      positionMode,
      marginType: marginMode,
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    return position;
  };

  private getOrderSideV2 = (opts: PlaceOrderOpts, positionMode: PositionMode) => {
    let side = opts.side === OrderSide.Buy ? 'buy' : 'sell';
    let tradeSide = undefined;
    if (positionMode === PositionMode.Hedged) {
      if (opts.reduceOnly) {
        tradeSide = 'close';
        side = opts.side === OrderSide.Buy ? 'sell' : 'buy';
      } else {
        tradeSide = 'open';
      }
    }
    return [side, tradeSide];
  };

  private getOrderSideUta = (
    opts: PlaceOrderOpts,
    positionMode: PositionMode,
  ): { side: 'buy' | 'sell'; posSide?: 'long' | 'short' } => {
    const side = opts.side === OrderSide.Buy ? 'buy' : 'sell';

    if (positionMode !== PositionMode.Hedged) {
      return { side };
    }

    if (opts.reduceOnly) {
      return {
        side,
        posSide: opts.side === OrderSide.Buy ? 'short' : 'long',
      };
    }

    return {
      side,
      posSide: opts.side === OrderSide.Buy ? 'long' : 'short',
    };
  };

  private getUtaStrategyOrderSide = (
    opts: PlaceOrderOpts,
    positionMode: PositionMode,
  ): { side: 'buy' | 'sell'; posSide?: 'long' | 'short' } => {
    const side = opts.side === OrderSide.Buy ? 'buy' : 'sell';

    if (positionMode !== PositionMode.Hedged) {
      return { side };
    }

    return {
      side,
      posSide: opts.side === OrderSide.Buy ? 'short' : 'long',
    };
  };

  private getUtaStrategyTriggerBy = (triggerType?: OrderStopTriggerType): 'market' | 'mark' => {
    return triggerType === OrderStopTriggerType.LastPrice ? 'market' : 'mark';
  };

  private getAlgoOrderSide = (opts: PlaceOrderOpts, positionMode: PositionMode) => {
    // For two-way (hedged) position mode
    if (positionMode === PositionMode.Hedged) {
      if (opts.type === OrderType.TakeProfit) {
        if (opts.side === OrderSide.Buy) return 'short';
        if (opts.side === OrderSide.Sell) return 'long';
      }

      if (opts.type === OrderType.StopLoss) {
        if (opts.side === OrderSide.Buy) return 'short';
        if (opts.side === OrderSide.Sell) return 'long';
      }

      if (opts.type === OrderType.TrailingStopLoss) {
        if (opts.side === OrderSide.Buy) return 'short';
        if (opts.side === OrderSide.Sell) return 'long';
      }
    }
    // For one-way position mode — Bitget V2 uses 'buy'/'sell' for holdSide
    else {
      if (opts.type === OrderType.TakeProfit) {
        if (opts.side === OrderSide.Buy) return 'sell';
        if (opts.side === OrderSide.Sell) return 'buy';
      }

      if (opts.type === OrderType.StopLoss) {
        if (opts.side === OrderSide.Buy) return 'sell';
        if (opts.side === OrderSide.Sell) return 'buy';
      }

      if (opts.type === OrderType.TrailingStopLoss) {
        if (opts.side === OrderSide.Buy) return 'sell';
        if (opts.side === OrderSide.Sell) return 'buy';
      }
    }

    throw new Error(`Unknown algo order side: ${opts.type} - ${opts.side}`);
  };

  private isVirtualTPSLOrder = (order: Order) => {
    return order.id.endsWith('__stop_loss') || order.id.endsWith('__take_profit');
  };

  private isAlgoOrder = (opts: PlaceOrderOpts) => {
    return (
      opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit || opts.type === OrderType.TrailingStopLoss
    );
  };

  // Get the correct planType for canceling an algo order.
  // Uses the raw planType from order.info (which comes from the API/WS response)
  // rather than computing it from position state, which is unreliable if the
  // position has been closed before the cancel request.
  private getCancelPlanType = (order: Order) => {
    const rawPlanType = order.info?.planType;
    if (rawPlanType) {
      // Map WS short codes to REST-compatible planType values
      const planTypeMap: Record<string, string> = {
        psl: 'pos_loss',
        ptp: 'pos_profit',
        pos_loss: 'pos_loss',
        pos_profit: 'pos_profit',
        loss_plan: 'loss_plan',
        profit_plan: 'profit_plan',
        track_plan: 'track_plan',
        sl: 'loss_plan',
        tp: 'profit_plan',
      };
      if (planTypeMap[rawPlanType]) return planTypeMap[rawPlanType];
    }

    // If the raw venue planType is missing, infer from immutable order fields
    // instead of live position state. A TP/SL can remain locally visible after
    // the protected position was closed by another trigger; recomputing against
    // flat positions downgrades a sized stop to pos_loss/pos_profit and emits a
    // misleading error before the user can clear the stale row.
    if (order.type === OrderType.TrailingStopLoss) return 'track_plan';
    const hasSizedAmount = this.planTypeComparableAmount(order) > 0;
    if (order.type === OrderType.StopLoss) return hasSizedAmount ? 'loss_plan' : 'pos_loss';
    if (order.type === OrderType.TakeProfit) return hasSizedAmount ? 'profit_plan' : 'pos_profit';

    const [planType] = this.selectPlanTypeAndAmount(order, this.store.positions);
    return planType;
  };

  // Positions used to decide sized-vs-position-level stops. The store is empty
  // on the never-started main-thread mirror that executes hosted-mode order
  // writes (see quirk 22), which silently downgraded every sized stop to a
  // position-level one. Fall back to a short-lived REST snapshot there.
  //
  // The store short-circuit asks the exact question the caller needs answered —
  // "is there a position on this symbol AND this side big enough to size the
  // stop?" — rather than the weaker "does the store know this symbol?". A
  // symbol-only check still downgraded stops whenever the store held some other
  // row for the symbol (e.g. a stale/zeroed long while the open position is
  // short): the answer looked available but the row that would justify a sized
  // stop was absent. Observed live as 4 buy-side stops going out as pos_loss
  // while sell-side ones on the same symbol were correctly sized.
  //
  // Only non-empty snapshots are cached, so a fetch taken while flat (the
  // entry+bracket flow formats algo payloads before the entry fills) cannot
  // pin an empty result for the rest of the TTL.
  // Deliberately NOT committed to the store — this is a read for one decision,
  // and writing it would pollute the mirror's streamed state.
  protected resolvePositionsForPlanType = async (symbol: string, side: PositionSide): Promise<Position[]> => {
    // Any open position on the protected side is enough — the stop is clamped
    // to it, so a store row smaller than the request no longer needs a REST
    // round-trip to confirm.
    const storeAnswers = this.store.positions.some((p) => p.symbol === symbol && p.side === side && p.contracts > 0);
    if (storeAnswers) return this.store.positions;

    const cached = this._planTypePositionsCache;
    if (cached && Date.now() - cached.at < BitgetExchange.POSITIONING_CACHE_TTL_MS) return cached.value;
    if (this._planTypePositionsInflight) return this._planTypePositionsInflight;

    const promise = (async () => {
      try {
        const positions = await this.fetchOpenPositions();
        if (positions.length > 0) {
          this._planTypePositionsCache = { at: Date.now(), value: positions };
        }
        return positions;
      } catch (err: any) {
        // Degrade to the pre-fix behavior (position-level stop) rather than
        // throwing: formatAlgoOrder runs inside placeOrders' Promise.all, so a
        // throw here would also block the unrelated entry order in a bracket.
        // Emit so the downgrade is visible instead of silent.
        this.emitAndLogError(
          `Could not load positions to size the ${symbol} stop — placing it as a full-position stop: ${
            err?.response?.data?.msg || err?.message
          }`,
        );
        return [];
      }
    })();

    this._planTypePositionsInflight = promise;
    try {
      return await promise;
    } finally {
      this._planTypePositionsInflight = null;
    }
  };

  // Compare against the size that will actually be POSTed, not the raw request.
  // formatAlgoOrder sends adjust(amount, precision.amount), so a "close the
  // whole position" stop derived from a percentage (e.g. 30.34 on a 30-contract
  // position with a lot size of 1) overshoots `contracts` by a fraction of a lot
  // and would silently downgrade to a position-level stop — the sent size is 30,
  // which fits perfectly. Observed live: 4 stops protecting a long went out as
  // pos_loss for exactly this reason.
  private planTypeComparableAmount = (opts: PlaceOrderOpts | Order): number => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market?.precision?.amount) return opts.amount;
    return adjust(opts.amount, market.precision.amount);
  };

  // `adjust` rounds to NEAREST, so it can land above the position it is meant to
  // bound. Step back one lot when it does, rather than `Math.floor(value / step)`
  // — that inherits IEEE division error and drops a whole lot on fractional
  // steps (0.29 / 0.01 = 28.999999999999996 → 0.28).
  private floorToLot = (symbol: string, value: number): number => {
    const step = this.store.marketsBySymbol[symbol]?.precision?.amount;
    if (!step) return value;
    const rounded = adjust(value, step);
    return rounded > value ? adjust(rounded - step, step) : rounded;
  };

  // amount === 0 is the product's "full position" sentinel (isFullStop) and must
  // keep mapping to pos_loss/pos_profit — do not size it from the position.
  protected selectPlanTypeAndAmount = (opts: PlaceOrderOpts | Order, positions: Position[]): [string, number] => {
    if (opts.type === OrderType.TrailingStopLoss) return ['track_plan', opts.amount];

    let sizedAmount: number | undefined;
    if (opts.amount !== 0) {
      const foundPosition = positions.find(
        (p) =>
          p.symbol === opts.symbol &&
          p.side === (opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) &&
          p.contracts > 0,
      );
      if (foundPosition) {
        // Clamp rather than downgrade: price drift routinely overshoots the
        // position by a fraction of a lot, and silently converting a stop the
        // user sized into a whole-position one is the worse failure.
        const clamped = Math.min(
          this.planTypeComparableAmount(opts),
          this.floorToLot(opts.symbol, foundPosition.contracts),
        );
        if (clamped > 0) sizedAmount = clamped;
      }
    }

    if (sizedAmount === undefined) {
      // Downgrading a sized stop is a real behavior change for the user: the
      // stop they sized closes the whole position instead. Say so, with the
      // numbers that drove the decision — a silent downgrade is what made this
      // take three rounds to pin down.
      if (opts.amount !== 0) {
        const candidates = positions
          .filter((p) => p.symbol === opts.symbol)
          .map((p) => `${p.side}:${p.contracts}`)
          .join(', ');
        this.emitAndLogError(
          `Sized ${opts.symbol} stop became a full-position stop: no open ${
            opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long
          } position to size ${this.planTypeComparableAmount(opts)} (requested ${opts.amount}) against, ` +
            `positions seen [${candidates || 'none'}]`,
        );
      }
      if (opts.type === OrderType.StopLoss) return ['pos_loss', 0];
      if (opts.type === OrderType.TakeProfit) return ['pos_profit', 0];
    } else {
      if (opts.type === OrderType.StopLoss) return ['loss_plan', sizedAmount];
      if (opts.type === OrderType.TakeProfit) return ['profit_plan', sizedAmount];
    }

    throw new Error(`Unknown order type: ${opts.type}`);
  };

  protected getOrderPlanTypeAndAmount = async (opts: PlaceOrderOpts | Order): Promise<[string, number]> => {
    const positions =
      opts.amount === 0 || opts.type === OrderType.TrailingStopLoss
        ? this.store.positions
        : await this.resolvePositionsForPlanType(
            opts.symbol,
            opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long,
          );

    return this.selectPlanTypeAndAmount(opts, positions);
  };
}
