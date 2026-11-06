import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';
import type { ExchangeName } from '../../../types';

import { sha256 } from '@noble/hashes/sha2.js';
import rateLimit from 'axios-rate-limit';
import BigNumber from 'bignumber.js';
import dayjs from 'dayjs';
import { toHex } from 'ethereum-cryptography/utils';
import chunk from 'lodash/chunk';
import orderBy from 'lodash/orderBy';
import uniqBy from 'lodash/uniqBy';
import { forEachSeries, mapSeries } from 'p-iteration';
import qs from 'qs';
import * as sdk from 'zklink-web-sdk-alpha';

import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Balance,
  Candle,
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
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { mergeUpdatesUsingLastUpdate } from '../../../utils/mergeUpdatesUsingLastUpdate';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, subtract } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './apexomni.api';
import {
  BROKER_ID,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TIME_IN_FORCE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
} from './apexomni.types';
import { ApexOmniPrivateWebsocket } from './apexomni.ws-private';
import { ApexOmniPublicWebsocket } from './apexomni.ws-public';
import { OrderWithClientId } from './interface/apexomni.index.interface';
import { AccountBalanceObject, UserObject } from './interface/apexomni.private.interface';
import { signContract, SignContractPayload } from './zklink/signer/transactions/contract';
import { initSdk } from './zklink/signer/utils';

// Helper functions from starkex-lib
const ONE_HOUR_MS = 60 * 60 * 1000;
const STARK_ORDER_SIGNATURE_EXPIRATION_BUFFER_HOURS = 24 * 7; // 1 week

/**
 * Convert an ISO timestamp to an epoch timestamp in hours, rounding up.
 */
function isoTimestampToEpochHours(isoTimestamp: string): number {
  return Math.ceil(new Date(Number(isoTimestamp)).getTime() / ONE_HOUR_MS);
}

/**
 * Add expiration buffer to ensure an order signature is valid when it arrives on-chain.
 */
function addOrderExpirationBufferHours(expirationEpochHours: number): number {
  return expirationEpochHours + STARK_ORDER_SIGNATURE_EXPIRATION_BUFFER_HOURS;
}

function d8(e = 16, t = 'D') {
  const n = '123456789',
    r = 'abcdef',
    i = t === 'hex' ? `${n}${r}` : n,
    s = t === 'hex' ? `0${n}${r}` : `0${n}`,
    c = t === 'hex' ? 15 : 9;
  let l = i[Math.floor(Math.random() * c)];
  for (let d = 1; d < e; d++) l += s[Math.floor(Math.random() * (c + 1))];
  return l;
}

function generateApeXRandomClientId(accountId: string) {
  return `apexomni-${accountId}-${Date.now()}-${d8(6)}`;
}

function feeHelper(e: string | number, t = 2, n = 'floor') {
  const r = Number(e),
    i = t === void 0 ? 2 : t;
  if (isNaN(r)) return Number(0).toFixed(i);
  {
    const s = Math.pow(10, i);
    return ((Math as any)[n](new BigNumber(r).multipliedBy(s).toNumber()) / s).toFixed(i);
  }
}

const UINT32_MAX = '4294967295';
const UINT64_MAX = '18446744073709551615';

const INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1_000;
const APEX_WS_EXECUTION_CACHE_LIMIT = 500;
const textEncoder = new TextEncoder();
const hashClientOrderId = (clientOrderId: string): Uint8Array => sha256(textEncoder.encode(clientOrderId));

export class ApexOmniExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('ApeX Omni', /^\S+$/);

  // Rate-limited xhr (5 RPS) for non-critical operations like balance/positions polling
  xhr: Axios;
  // Unlimited xhr for critical time-sensitive operations (order placement, cancellation, leverage changes)
  // This prevents background polling from blocking urgent order operations
  unlimitedXHR: Axios;

  publicWebsocket: ApexOmniPublicWebsocket;
  privateWebsocket: ApexOmniPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private unifiedMarginStatus: number = 1;

  private networkId: number = 1;

  lastWebsocketPositionsUpdate: number | undefined;

  // Track recently cancelled orders to prevent re-adding them before API processes cancellation
  recentlyCancelledOrders: Map<string, number> = new Map();
  recentWsExecutions: Execution[] = [];
  private executionIdentities = new WeakMap<Execution, string>();
  private executionTimestamps = new WeakMap<Execution, number>();
  private syntheticExecutions = new WeakSet<Execution>();
  private seenWsExecutions = new Set<string>();
  private seenWsExecutionQueue: string[] = [];

  get accountType() {
    return this.unifiedMarginStatus === 1 ? 'CONTRACT' : 'UNIFIED';
  }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 5 });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new ApexOmniPublicWebsocket(this);
    this.privateWebsocket = new ApexOmniPrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  getAccount = async () => {
    const { data } = await this.xhr.get<{ data: { id: string } }>(ENDPOINTS.ACCOUNT);

    return {
      userId: data.data.id,
      // ApeX Omni's documented v3 account response has no inviter/referrer field.
      affiliateId: null,
    };
  };

  validateAccount = async () => {
    try {
      await this.xhr.get<UserObject>(ENDPOINTS.USER);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }

    try {
      await initSdk();

      const signer = sdk.newRpcSignerWithProvider({});
      await signer.initZklinkSigner(this.options.omniKeySeed);
    } catch (_e) {
      return { error: 'Invalid OmniKey seed' };
    }

    return { error: '' };
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    if (!useExternalPublicData) {
      // load initial market data
      // then we can poll for live data
      await Promise.all([
        this.fetchMarkets().then((markets) => {
          if (this.isDisposed) return [];
          this.log(`Loaded ${markets.length} ApeX Pro markets`);
          this.store.update({
            markets,
            tickers: markets.map((market) => ({
              id: market.id,
              symbol: market.symbol,
              bid: 0,
              ask: 0,
              last: 0,
              volume: 0,
              quoteVolume: 0,
              mark: 0,
              index: 0,
              percentage: 0,
              openInterest: 0,
              fundingRate: 0,
              fundingTime: 0,
            })),
            loaded: { ...this.store.loaded, markets: true, tickers: true },
          });
          return markets;
        }),
      ]);

      if (this.isDisposed) return;

      // start websocket streams
      this.publicWebsocket.connectAndSubscribe();
    }

    this.log(`Ready to trade on ApeX Pro`);

    // start ticking live data
    // balance, tickers, positions
    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.store.update({ options: { ...this.store.options, isHedged: true } });
    this.privateWebsocket.connectAndSubscribe();

    try {
      const [, balance] = await Promise.all([
        this.fetchPositions().then((positions) => {
          if (this.isDisposed) return [];
          this.store.addOrUpdatePositionsFull(positions);
          this.store.update({
            loaded: { ...this.store.loaded, positions: true, orders: true },
          });
          return positions;
        }),
        this.fetchRawBalance(),
      ]);

      if (this.isDisposed) return;
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err);
    }
    this.tick();
  };

  fetchAndSetPrivateSnapshot = async () => {
    await this.fetchAndSetPrivateSnapshotHelper();
  };

  fetchAndSetPrivateSnapshotHelper = async () => {
    try {
      const [fetchedPositions, fetchedOrders] = await Promise.all([this.fetchPositions(), this.fetchOrders()]);

      if (this.isDisposed) return;

      const positions =
        this.lastWebsocketPositionsUpdate !== undefined &&
        virtualClock.getCurrentTimeMs() - this.lastWebsocketPositionsUpdate <
          INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY
          ? mergeUpdatesUsingLastUpdate(fetchedPositions, this.store.positions)
          : fetchedPositions;

      // Update positions - note this is now safe because we're also fetching orders
      this.store.addOrUpdatePositionsFull(positions);
      this.store.addOrUpdateOrdersFull(fetchedOrders);

      this.store.update({
        loaded: { ...this.store.loaded, positions: true, orders: true },
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

  tick = async () => {
    if (!this.isDisposed) {
      try {
        await this.fetchAndSetPrivateSnapshotHelper();
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      loop(() => this.tick(), this.getTickInterval());
    }
  };

  fetchBalance = async () => {
    const rawBalance = await this.fetchRawBalance();
    if (this.isDisposed) return this.store.balance;
    return rawBalance;
  };

  fetchRawBalance = async (): Promise<Balance> => {
    const { data } = await this.xhr.get(ENDPOINTS.BALANCE);

    // The cast below is a LIE whenever the venue answers with anything but the
    // balance envelope — an error body, a throttle, a truncated response — and
    // apexomni does that on roughly a fifth of its calls (measured 2026-08-17).
    // Dereferencing it produced `Cannot read properties of undefined (reading
    // 'totalEquityValue')` in production, from inside a worker, so
    // the stack pointed at the Sentry sanitiser rather than at this line.
    //
    // Returning the LAST KNOWN balance, not a zeroed one. A fabricated zero
    // equity is the `exitPrice: 0` failure in a place a user acts on: it renders
    // as a wiped account and feeds margin display. Truth-as-of-last-success is
    // the honest answer for a transient venue hiccup, and the poll retries in
    // seconds. Logged rather than thrown so it stays visible without spamming
    // every user for something that self-heals.
    if (!data?.data) {
      this.log(`Balance response carried no account payload; keeping the last known balance`, LogSeverity.Warning);
      return this.store.balance;
    }

    const balance = data.data as AccountBalanceObject;
    const totalEquity = parseFloat(balance.totalEquityValue);
    const totalUsed = parseFloat(balance.maintenanceMargin);
    const initialMargin = parseFloat(balance.initialMargin);
    if (![totalEquity, totalUsed, initialMargin].every(Number.isFinite)) {
      this.log(
        'Balance response carried invalid financial values; keeping the last known balance',
        LogSeverity.Warning,
      );
      return this.store.balance;
    }

    // this is no longer compatible with our approach to USDC pnl. Need to sum up position pnl
    // const upnl = parseFloat(firstAccount.totalPerpUPL);
    const upnl = this.store.positions.reduce((acc, p) => {
      const ticker = this.store.tickersBySymbol[p.symbol];

      if (!ticker) {
        return acc;
      }

      return (
        acc + (p.side === PositionSide.Long ? ticker!.last - p.entryPrice : p.entryPrice - ticker!.last) * p.contracts
      );
    }, 0);

    const total = subtract(totalEquity, upnl);

    return {
      total,
      upnl,
      used: +totalUsed,
      free: totalEquity - initialMargin,
      info: balance,
    };
  };

  // side effect - balance update
  fetchPositions = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT);
    if (!Array.isArray(data?.data?.positions)) {
      throw new Error('ApeX account response carried no valid positions snapshot');
    }
    const positions: Position[] = data.data.positions
      .map(this.mapPosition.bind(this))
      .filter((p: Position | null): p is Position => p !== null);

    if (!this.store.zkLinkConfig?.l2Key) {
      this.store.updateZkLinkConfig({
        ...this.store.zkLinkConfig,
        l2Key: data.data.l2key,
        ethereumAddress: data.data.ethereumAddress,
        id: data.data.id,
        spotAccount: data.data.spotAccount,
        spotWallets: data.data.spotWallets,
        contractAccount: data.data.contractAccount,
        contractWallets: data.data.contractWallets,
      });
    }

    // data.data.accounts?.forEach((account: any) => {
    //   takerFeePerToken[account.token as 'USDT' | 'USDC'] = parseFloat(account.takerFeeRate);
    //
    //   this.store.updateStarkConfig({ ...this.store.starkConfig, positionId: data.data.positionId });
    // });
    //
    // this.store.updateStarkConfig({ ...this.store.starkConfig, takerFeePerToken });

    const fakePositions: Position[] = this.store.markets.reduce((acc: Position[], m) => {
      const fakePositionsForSymbol = [
        {
          id: createPositionId(m.symbol, PositionSide.Long, PositionMode.Hedged),
          symbol: m.symbol,
          side: PositionSide.Long,
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          leverage: 1 / +m.info?.initialMarginRate || 10,
          liquidationPrice: 0,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
        },
        {
          id: createPositionId(m.symbol, PositionSide.Short, PositionMode.Hedged),
          symbol: m.symbol,
          side: PositionSide.Short,
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          leverage: 1 / +m.info?.initialMarginRate || 10,
          liquidationPrice: 0,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
        },
      ];

      acc.push(...fakePositionsForSymbol);
      return acc;
    }, []);

    return uniqBy([...positions, ...fakePositions], 'id');
  };

  fetchOrders = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.OPEN_ORDERS);
    if (!Array.isArray(data?.data)) {
      throw new Error('ApeX open-orders response carried no valid orders snapshot');
    }
    const orders = data.data;
    const now = Date.now();

    // Clean up expired cancelled orders (older than 10 seconds)
    for (const [orderId, timestamp] of this.recentlyCancelledOrders.entries()) {
      if (now - timestamp > 10_000) {
        this.recentlyCancelledOrders.delete(orderId);
      }
    }

    // Filter out recently cancelled orders
    const filteredOrders = orders.filter((order: any) => {
      return !this.recentlyCancelledOrders.has(order.id);
    });

    const mappedOrders = filteredOrders.flatMap((order: any) => this.mapOrder(order));

    return mappedOrders;
  };

  private normalizeApexSymbolKey = (value: unknown) =>
    String(value || '')
      .replace(/[^a-zA-Z0-9]/g, '')
      .toUpperCase()
      .replace(/^PERP/, '')
      .replace(/PERP$/, '')
      .replace(/SWAP$/, '');

  private normalizeExecutionTimestampToMilliseconds = (value: unknown) => {
    if (!['number', 'string'].includes(typeof value) || `${value}`.trim() === '') return undefined;
    const numeric = Number(value);
    const timestamp = Number.isFinite(numeric)
      ? numeric > 1_000_000_000_000
        ? numeric
        : numeric * 1000
      : typeof value === 'string'
        ? Date.parse(value)
        : NaN;
    return Number.isFinite(timestamp) && timestamp > 0 ? timestamp : undefined;
  };

  private executionTimestamp = (execution: Execution) =>
    this.executionTimestamps.get(execution) ?? execution.timestamp * 1000;

  private executionIdentity = (execution: Execution) =>
    this.executionIdentities.get(execution) ||
    `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;

  private tagExecution = (
    execution: Execution,
    row: Record<string, any>,
    source: 'socket' | 'fills' | 'orders' | 'synthetic',
  ) => {
    const candidates =
      source === 'synthetic'
        ? []
        : source === 'socket'
          ? [row.id, row.matchFillId]
          : [row.tradeId, row.tradeID, row.fillId, row.execId, row.id, row.i];
    const id = candidates.find(
      (candidate) => candidate !== undefined && candidate !== null && `${candidate}`.length > 0,
    );
    const time =
      row.createdAt ||
      row.fillTime ||
      row.createdTime ||
      row.timestamp ||
      row.ts ||
      row.timeStamp ||
      row.time ||
      row.updatedAt ||
      row.updatedTime ||
      row.tradeTime ||
      row.transactTime ||
      row.E ||
      row.t ||
      row.T;
    const identity =
      source === 'synthetic'
        ? `order:${execution.orderId || 'no-order'}:${row.cumulative || execution.qty}:${time || execution.timestamp}`
        : id !== undefined
          ? `execution:${id}`
          : `${execution.orderId || 'no-order'}:${time || execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
    this.executionIdentities.set(execution, identity);
    const timestamp = this.normalizeExecutionTimestampToMilliseconds(time);
    if (timestamp !== undefined) this.executionTimestamps.set(execution, timestamp);
    if (source === 'synthetic') this.syntheticExecutions.add(execution);
    return identity;
  };

  consumeSyntheticExecutions = (orderId: string, amount: number) => {
    let remaining = amount;
    this.recentWsExecutions = this.recentWsExecutions.flatMap((execution) => {
      if (!this.syntheticExecutions.has(execution) || execution.orderId !== orderId || remaining <= 0) {
        return [execution];
      }
      const reduction = Math.min(remaining, execution.qty);
      remaining -= reduction;
      if (reduction === execution.qty) return [];
      const updated = {
        ...execution,
        qty: execution.qty - reduction,
        commission: (execution.commission || 0) * ((execution.qty - reduction) / execution.qty),
      };
      this.executionIdentities.set(updated, this.executionIdentity(execution));
      this.executionTimestamps.set(updated, this.executionTimestamp(execution));
      this.syntheticExecutions.add(updated);
      return [updated];
    });
  };

  recordWsExecution = (row: Record<string, any>, synthetic = false): Execution | undefined => {
    const rawRowSymbol =
      row.symbol || row.s || row.instId || row.instrument || row.market || row.crossSymbolName || row.marketSymbol;
    if (!rawRowSymbol) {
      return;
    }

    const rowSymbol = String(rawRowSymbol);
    const normalizedRowSymbol = this.normalizeApexSymbolKey(rowSymbol);
    const resolvedMarket =
      this.store.marketsById[rowSymbol] ||
      this.store.marketsBySymbol[rowSymbol] ||
      this.store.markets.find((market) => {
        return (
          this.normalizeApexSymbolKey(market.id) === normalizedRowSymbol ||
          this.normalizeApexSymbolKey(market.symbol) === normalizedRowSymbol
        );
      });
    if (!resolvedMarket) return;

    const normalizedSide = String(row.side || row.S || '')
      .trim()
      .toUpperCase();
    const side =
      normalizedSide === 'BUY' || normalizedSide === 'B' || normalizedSide === 'LONG'
        ? OrderSide.Buy
        : normalizedSide === 'SELL' || normalizedSide === 'S' || normalizedSide === 'SHORT'
          ? OrderSide.Sell
          : ORDER_SIDE[normalizedSide];
    if (!side) {
      return;
    }

    const timestampMs = this.normalizeExecutionTimestampToMilliseconds(
      row.createdAt ||
        row.fillTime ||
        row.createdTime ||
        row.timestamp ||
        row.ts ||
        row.timeStamp ||
        row.time ||
        row.updatedAt ||
        row.updatedTime ||
        row.tradeTime ||
        row.transactTime ||
        row.E ||
        row.t ||
        row.T,
    );
    if (timestampMs === undefined) {
      return;
    }

    const price = Number.parseFloat(String(row.price || row.p || row.fillPrice || row.fillPx || row.execPrice || '0'));
    const qty = Number.parseFloat(
      String(
        row.fillSize ||
          row.execQty ||
          row.matchFillSize ||
          row.lastFillSize ||
          row.qty ||
          row.q ||
          row.size ||
          row.v ||
          row.amount ||
          '0',
      ),
    );
    if (!Number.isFinite(price) || !Number.isFinite(qty) || price <= 0 || qty <= 0) {
      return;
    }

    const orderId = String(row.orderId || row.orderID || row.id || row.i || row.clientOrderId || row.clientId || '');
    const fee = Number.parseFloat(
      String(row.fee || row.fillFee || row.execFee || row.cumSuccessFillFee || row.cumMatchFillFee || '0'),
    );

    const execution = omitUndefined({
      timestamp: Math.floor(timestampMs / 1000),
      symbol: resolvedMarket?.symbol || rowSymbol,
      orderId: orderId || undefined,
      price,
      qty,
      side,
      commission: Number.isFinite(fee) ? Math.abs(fee) : undefined,
      commissionAsset:
        row.token || row.feeToken || row.feeAsset || row.feeCurrency || row.settleAssetId || resolvedMarket?.quote,
    }) as Execution;

    const executionKey = this.tagExecution(execution, row, synthetic ? 'synthetic' : 'socket');
    if (!synthetic) {
      if (this.seenWsExecutions.has(executionKey)) return;
      this.seenWsExecutions.add(executionKey);
      this.seenWsExecutionQueue.push(executionKey);
      if (this.seenWsExecutionQueue.length > 8192) {
        this.seenWsExecutions.delete(this.seenWsExecutionQueue.shift()!);
      }
    }
    this.recentWsExecutions = [
      execution,
      ...this.recentWsExecutions.filter((item) => this.executionIdentity(item) !== executionKey),
    ]
      .sort((a, b) => this.executionTimestamp(b) - this.executionTimestamp(a))
      .slice(0, APEX_WS_EXECUTION_CACHE_LIMIT);
    return execution;
  };

  private getWsExecutionFallback = ({
    symbol,
    orderId,
    fromMilliseconds,
    toMilliseconds,
    limit,
  }: {
    symbol: string;
    orderId?: string;
    fromMilliseconds?: number;
    toMilliseconds?: number;
    limit: number;
  }): Execution[] => {
    const expectedSymbolKey = this.normalizeApexSymbolKey(symbol);
    return this.recentWsExecutions
      .filter((execution) => this.normalizeApexSymbolKey(execution.symbol) === expectedSymbolKey)
      .filter((execution) => {
        if (orderId && execution.orderId && execution.orderId !== String(orderId)) {
          return false;
        }
        if (fromMilliseconds !== undefined && this.executionTimestamp(execution) < fromMilliseconds) {
          return false;
        }
        if (toMilliseconds !== undefined && this.executionTimestamp(execution) > toMilliseconds) {
          return false;
        }
        return true;
      })
      .sort((a, b) => this.executionTimestamp(b) - this.executionTimestamp(a))
      .slice(0, limit);
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
    const executionHistoryXHR = this.xhr || this.unlimitedXHR;
    const configuredExecutionHistoryTimeoutMs = Number(
      (this.options.extra as Record<string, unknown> | undefined)?.executionHistoryTimeoutMs,
    );
    const executionHistoryTimeoutMs =
      Number.isFinite(configuredExecutionHistoryTimeoutMs) && configuredExecutionHistoryTimeoutMs > 0
        ? configuredExecutionHistoryTimeoutMs
        : 20_000;

    const normalizeToMilliseconds = (value?: number) => {
      if (value === undefined || value === null || !Number.isFinite(value)) {
        return undefined;
      }
      return value > 1_000_000_000_000 ? Math.floor(value) : Math.floor(value * 1000);
    };

    const fromMilliseconds = normalizeToMilliseconds(from);
    const toMilliseconds = normalizeToMilliseconds(to);
    const symbolCandidates = Array.from(new Set([market.id, market.symbol].filter((candidate) => Boolean(candidate))));
    // Omni v3 docs/SDK primarily use symbol + pagination for fills history.
    // Keep token omitted by default to avoid strict-auth/signature drifts on some environments.
    const tokenCandidates: Array<string | undefined> = [undefined];

    const firstDefined = (...values: Array<unknown>) =>
      values.find((value) => value !== undefined && value !== null && `${value}`.length > 0);
    const mapExecutionSide = (value: unknown): OrderSide | undefined => {
      const normalized = String(value || '')
        .trim()
        .toUpperCase();
      if (!normalized) {
        return undefined;
      }
      if (normalized === 'BUY' || normalized === 'B' || normalized === 'LONG') {
        return OrderSide.Buy;
      }
      if (normalized === 'SELL' || normalized === 'S' || normalized === 'SHORT') {
        return OrderSide.Sell;
      }
      return ORDER_SIDE[normalized];
    };

    const expectedSymbolKeys = new Set([
      this.normalizeApexSymbolKey(market.id),
      this.normalizeApexSymbolKey(market.symbol),
    ]);

    const parseRows = (rows: Array<Record<string, any>>, isFallback = false) =>
      rows.reduce<Execution[]>((acc, row) => {
        const rawRowSymbol =
          row.symbol || row.s || row.instId || row.instrument || row.market || row.crossSymbolName || row.marketSymbol;
        if (rawRowSymbol !== undefined && rawRowSymbol !== null && `${rawRowSymbol}`.length > 0) {
          const rowSymbol = String(rawRowSymbol);
          const resolvedRowMarket = this.store.marketsById[rowSymbol] || this.store.marketsBySymbol[rowSymbol];
          const normalizedRowSymbol = this.normalizeApexSymbolKey(
            resolvedRowMarket?.id || resolvedRowMarket?.symbol || rowSymbol,
          );
          if (!expectedSymbolKeys.has(normalizedRowSymbol)) {
            return acc;
          }
        }

        const side = mapExecutionSide(row.side || row.S);
        if (!side) {
          return acc;
        }

        const cumulativeFillValue = parseFloat(String(row.cumSuccessFillValue || row.cumMatchFillValue || '0'));
        const cumulativeFillSize = parseFloat(String(row.cumSuccessFillSize || row.cumMatchFillSize || '0'));
        const derivedPrice =
          Number.isFinite(cumulativeFillValue) && Number.isFinite(cumulativeFillSize) && cumulativeFillSize > 0
            ? cumulativeFillValue / cumulativeFillSize
            : undefined;
        const price = parseFloat(
          String(
            firstDefined(
              row.fillPrice,
              row.fillPx,
              row.execPrice,
              row.latestMatchFillPrice,
              row.avgPrice,
              row.price,
              row.p,
              derivedPrice,
              '0',
            ) || '0',
          ),
        );
        const qty = parseFloat(
          String(
            firstDefined(
              isFallback ? row.cumSuccessFillSize : undefined,
              row.fillSize,
              row.execQty,
              row.matchFillSize,
              row.lastFillSize,
              row.qty,
              row.q,
              row.amount,
              row.size,
              row.v,
              row.cumSuccessFillSize,
              row.cumMatchFillSize,
              '0',
            ) || '0',
          ),
        );
        if (!Number.isFinite(price) || !Number.isFinite(qty) || price <= 0 || qty <= 0) {
          return acc;
        }

        const timestampMs = this.normalizeExecutionTimestampToMilliseconds(
          row.createdAt ||
            row.fillTime ||
            row.createdTime ||
            row.timestamp ||
            row.ts ||
            row.timeStamp ||
            row.time ||
            row.updatedAt ||
            row.updatedTime ||
            row.tradeTime ||
            row.transactTime ||
            row.E ||
            row.t ||
            row.T,
        );
        if (timestampMs === undefined) {
          return acc;
        }

        const executionOrderId = String(
          row.orderId || row.orderID || row.id || row.i || row.clientOrderId || row.clientId || '',
        );
        if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
          return acc;
        }

        const fee = parseFloat(
          String(
            firstDefined(row.fee, row.fillFee, row.execFee, row.cumSuccessFillFee, row.cumMatchFillFee, '0') || '0',
          ),
        );
        const execution = omitUndefined({
          timestamp: Math.floor(timestampMs / 1000),
          symbol: market.symbol,
          orderId: executionOrderId || undefined,
          price,
          qty,
          side,
          commission: Number.isFinite(fee) ? Math.abs(fee) : undefined,
          commissionAsset:
            row.token || row.feeToken || row.feeAsset || row.feeCurrency || row.settleAssetId || market.quote,
        }) as Execution;
        this.tagExecution(execution, row, isFallback ? 'orders' : 'fills');
        acc.push(execution);
        return acc;
      }, []);

    const extractRows = (payload: any): Array<Record<string, any>> => {
      const body = payload?.body ?? payload?.data ?? payload;
      const root = body?.data ?? body?.result ?? body;
      const candidates = [root?.fills, root?.orders, root?.list, root?.rows, root?.results, root];
      for (const candidate of candidates) {
        if (Array.isArray(candidate)) {
          return candidate;
        }
      }
      return [];
    };

    const fetchFillRows = async () => {
      const maxPages = fromMilliseconds || toMilliseconds ? 3 : 1;
      let lastError: any;

      for (const symbolCandidate of symbolCandidates) {
        for (const tokenCandidate of tokenCandidates) {
          const rows: Array<Record<string, any>> = [];
          let page = 0;

          try {
            while (page < maxPages && rows.length < maxLimit) {
              const response = await executionHistoryXHR.get(ENDPOINTS.FILLS, {
                timeout: executionHistoryTimeoutMs,
                params: omitUndefined({
                  page: page > 0 ? page : undefined,
                  limit: maxLimit,
                  symbol: symbolCandidate,
                  token: tokenCandidate,
                  beginTimeInclusive: fromMilliseconds,
                  endTimeExclusive: toMilliseconds === undefined ? undefined : toMilliseconds + 1,
                  side: undefined,
                }),
              } as any);
              const pageRows = extractRows(response?.data);
              if (!pageRows.length) {
                break;
              }
              rows.push(...pageRows);
              if (pageRows.length < maxLimit) {
                break;
              }
              page += 1;
            }

            if (rows.length > 0) {
              return rows;
            }
          } catch (err: any) {
            lastError = err;
          }
        }
      }

      if (lastError) {
        throw lastError;
      }

      return [];
    };

    const fetchHistoryOrdersFallback = async () => {
      let lastError: any;

      for (const symbolCandidate of symbolCandidates) {
        for (const tokenCandidate of tokenCandidates) {
          try {
            const response = await executionHistoryXHR.get(ENDPOINTS.HISTORY_ORDERS, {
              timeout: executionHistoryTimeoutMs,
              params: omitUndefined({
                page: undefined,
                limit: maxLimit,
                symbol: symbolCandidate,
                token: tokenCandidate,
                orderType: 'HISTORY',
                beginTimeInclusive: fromMilliseconds,
                endTimeExclusive: toMilliseconds === undefined ? undefined : toMilliseconds + 1,
              }),
            } as any);
            const rows = extractRows(response?.data).filter((row) => parseFloat(row.cumSuccessFillSize || '0') > 0);
            if (rows.length > 0) {
              return rows;
            }
          } catch (err: any) {
            lastError = err;
          }
        }
      }

      if (lastError) {
        throw lastError;
      }

      return [];
    };

    const getWsFallbackExecutions = () =>
      this.getWsExecutionFallback({
        symbol: market.symbol,
        orderId,
        fromMilliseconds,
        toMilliseconds,
        limit: maxLimit,
      });

    try {
      let primaryRows: Array<Record<string, any>> = [];
      try {
        primaryRows = await fetchFillRows();
      } catch (primaryErr: any) {
        const primaryErrorMessage = primaryErr?.response?.data?.msg || primaryErr?.message || String(primaryErr);
        this.log(
          `Execution history fills request failed, falling back to history-orders: ${primaryErrorMessage}`,
          LogSeverity.Warning,
        );
      }
      let fallbackRows: Array<Record<string, any>> = [];
      if (primaryRows.length === 0) {
        try {
          fallbackRows = await fetchHistoryOrdersFallback();
        } catch (fallbackErr: any) {
          throw fallbackErr;
        }
      }

      const parsedPrimaryRows = parseRows(primaryRows);
      const parsedFallbackRows = parseRows(fallbackRows, true);
      const restExecutions = [...parsedPrimaryRows, ...parsedFallbackRows];
      // Only fall back to the WS in-memory cache when REST returned nothing.
      // The WS cache and REST execution rows have different timestamp/qty granularity
      // (delta vs cumulative-derived), so merging them produces phantom duplicate fills
      // on the chart and overcounts on the fills table when REST is already authoritative.
      const wsFallbackExecutions = restExecutions.length === 0 ? getWsFallbackExecutions() : [];
      const deduped = new Map<string, Execution>();

      for (const execution of [...restExecutions, ...wsFallbackExecutions]) {
        if (fromMilliseconds !== undefined && this.executionTimestamp(execution) < fromMilliseconds) {
          continue;
        }
        if (toMilliseconds !== undefined && this.executionTimestamp(execution) > toMilliseconds) {
          continue;
        }
        const key = this.executionIdentity(execution);
        deduped.set(key, execution);
      }
      return [...deduped.values()]
        .sort((a, b) => this.executionTimestamp(b) - this.executionTimestamp(a))
        .slice(0, maxLimit);
    } catch (err: any) {
      const message = err?.response?.data?.msg || err?.message || String(err);
      const wsFallbackExecutions = getWsFallbackExecutions();
      if (wsFallbackExecutions.length > 0) {
        this.log(
          `Execution history fetch failed (${message}), serving ${wsFallbackExecutions.length} cached ws fills`,
          LogSeverity.Warning,
        );
        return wsFallbackExecutions;
      }
      this.log(`Execution history fetch failed: ${message}`, LogSeverity.Warning);
      return [];
    }
  };

  fetchMarkets = async () => {
    const markets = await this.fetchMarketsHelper();
    if (markets.some((m) => m === null)) {
      this.emitAndLogError('Failed to fetch markets');
      return this.store.markets;
    }
    return markets as Market[];
  };

  fetchMarketsHelper = async () => {
    const {
      data: {
        data: { contractConfig },
      },
    } = await this.xhr.get<Record<string, any>>(ENDPOINTS.SYMBOLS);

    this.store.updateZkLinkConfig({
      ...this.store.zkLinkConfig,
      contractConfig,
    });
    // {
    //   "baselinePositionValue": "50000",
    //   "crossId": 30002,
    //   "crossSymbolId": 19,
    //   "crossSymbolName": "ENSUSDT",
    //   "digitMerge": "0.001,0.002,0.004,0.01,0.02",
    //   "displayMaxLeverage": "50",
    //   "displayMinLeverage": "1",
    //   "enableDisplay": true,
    //   "enableOpenPosition": true,
    //   "enableTrade": true,
    //   "fundingImpactMarginNotional": "5000",
    //   "fundingInterestRate": "0.0003",
    //   "incrementalInitialMarginRate": "0.01",
    //   "incrementalMaintenanceMarginRate": "0.01",
    //   "incrementalPositionValue": "50000",
    //   "initialMarginRate": "0.02000",
    //   "maintenanceMarginRate": "0.01",
    //   "maxOrderSize": "1000",
    //   "maxPositionSize": "2000",
    //   "minOrderSize": "0.1",
    //   "maxMarketPriceRange": "0.045",
    //   "settleAssetId": "USDT",
    //   "baseTokenId": "ENS",
    //   "stepSize": "0.1",
    //   "symbol": "ENS-USDT",
    //   "symbolDisplayName": "ENSUSDT",
    //   "tickSize": "0.001",
    //   "maxMaintenanceMarginRate": "0.50",
    //   "maxPositionValue": "4000000",
    //   "tagIconUrl": "",
    //   "tag": "",
    //   "riskTip": false,
    //   "defaultInitialMarginRate": "0.05",
    //   "klineStartTime": 0,
    //   "maxMarketSizeBuffer": "0.98",
    //   "enableFundingSettlement": true,
    //   "indexPriceDecimals": 3,
    //   "indexPriceVarRate": "0.001",
    //   "openPositionOiLimitRate": "0.05",
    //   "fundingMaxRate": "0.000305",
    //   "fundingMinRate": "-0.000305",
    //   "fundingMaxValue": "",
    //   "enableFundingMxValue": true,
    //   "l2PairId": "50033",
    //   "settleTimeStamp": 0,
    //   "isPrelaunch": false,
    //   "riskLimitConfig": {
    //   "positionSteps": [
    //     "0",
    //     "50000",
    //     "200000",
    //     "300000",
    //     "500000",
    //     "1000000",
    //     "2000000",
    //     "3000000",
    //     "4000000"
    //   ],
    //     "imrSteps": [
    //     "0.02000",
    //     "0.04000",
    //     "0.05000",
    //     "0.10000",
    //     "0.20000",
    //     "0.25000",
    //     "0.33333",
    //     "0.50000",
    //     "1.00000"
    //   ],
    //     "mmrSteps": [
    //     "0.01",
    //     "0.015",
    //     "0.025",
    //     "0.05",
    //     "0.10",
    //     "0.125",
    //     "0.1665",
    //     "0.25",
    //     "0.50"
    //   ]
    // }
    // }
    const markets: Market[] = contractConfig.perpetualContract
      .filter((market: Record<string, any>) => market.enableTrade)
      .map((market: any) => {
        return {
          // old id isn't unique with expiring usdc futs
          // id: `${market.baseCoin}/${market.quoteCoin}:${market.settleCoin}`,
          id: market.symbol,
          symbol: market.crossSymbolName,
          base: market.baseTokenId,
          quote: market.settleAssetId,
          active: market.enableTrade,
          precision: {
            amount: parseFloat(market.stepSize),
            price: parseFloat(market.tickSize),
          },
          limits: {
            amount: {
              min: parseFloat(market.minOrderSize),
              max: parseFloat(market.maxOrderSize),
            },
            leverage: {
              min: parseFloat(market.displayMinLeverage),
              max: parseFloat(market.displayMaxLeverage),
            },
          },
          isExpiring: false,
          info: market,
        };
      });

    return markets;
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 500, 1500);
    const interval = INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const hypotheticalEnd = opts.to ? dayjs(opts.to) : dayjs();
    const end = interval === 'D' ? undefined : hypotheticalEnd;
    const start =
      !opts.limit && opts.from
        ? dayjs(opts.from)
        : hypotheticalEnd.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const params = {
      symbol: opts.symbol,
      start: start.unix(),
      interval,
      //limit,
      ...(end ? { end: end.unix() } : {}),
    };

    const { data } = await this.xhr.get(ENDPOINTS.KLINE, { params });

    if (!data?.data?.[opts.symbol]) {
      console.error(`No data for symbol ${opts.symbol}`, data);
      return [];
    }

    const candles: Candle[] = orderBy(
      data?.data?.[opts.symbol].map?.(
        (data: { t: number; s: string; l: string; h: string; o: string; c: string; v: string }) => {
          return {
            timestamp: data.t / 1000,
            open: parseFloat(data.o),
            high: parseFloat(data.h),
            low: parseFloat(data.l),
            close: parseFloat(data.c),
            volume: parseFloat(data.v),
          };
        },
      ),
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

  /*
    STOP for position

    clientId: 11120710676290946
    expiration: 1726484400000
    isPositionTpsl: true
    limitFee: 0.000001
    price: 0.5
    reduceOnly: true
    side: SELL
    signature: 04f0e40861dbed9f7f012beb7dc104fa0f58e0b42250a2a16c934838b3130e13058c66145c387be6d496408a94bf63cf2869dc7a40cfd5c5c8beeca942b9de21
    size: 0.002
    symbol: BTC-USDC
    timeInForce: GOOD_TIL_CANCEL
    triggerPrice: 56500.0
    triggerPriceType: INDEX
    type: STOP_MARKET

    MARKET order

    clientId: 16903693475780246
    expiration: 1726484400000
    limitFee: 0.032551
    price: 59182.0
    reduceOnly: false
    side: BUY
    signature: 04a249a9a0a1398f05802c6bf6b516a8a6ab698fc29b28ff5ee88443f13d337a00ad32bec2790067704ea9cd7ea2cd710e5d67898681d34afe18aee307fffdad
    size: 0.001
    symbol: BTC-USDC
    type: MARKET

    LIMIT with TP/SL

    clientId: 522530814653728
    expiration: 1726484400000
    limitFee: 0.026500
    price: 53000.0
    reduceOnly: false
    side: BUY
    signature: 000c3ca90f10e5586bdc8c03d563b968ffaff146dd8263feca5b5a7de317b27c03c10ecce4d7232b81c220387b41703be06817da268275ec777d1659db852f87
    size: 0.001
    symbol: BTC-USDC
    timeInForce: GOOD_TIL_CANCEL
    type: LIMIT

    isOpenTpslOrder: true
    isSetOpenSl: true
    isSetOpenTp: true

    slClientOrderId: 53874026834695776
    slExpiration: 1726484400000
    slLimitFee: 0.000001
    slPrice: 0.5
    slSide: SELL
    slSignature: 003a086bd54c69e3e7530b7910f7dc6b04e7fda484589388763194d85bb68efe011ef4d3e5a13ac42b666e9e29ffe6be850e39a8887b1cfc47921f3064c58830
    slSize: 0.001
    slTriggerPrice: 24000
    slTriggerPriceType: INDEX

    tpClientOrderId: 6235928390937044
    tpExpiration: 1726484400000
    tpLimitFee: 0.000001
    tpPrice: 0.5
    tpSide: SELL
    tpSignature: 06bf76fed3faa0f77982b61797ff4a7561c1a216e55cf45c9ca29c874132a80404cdab25568799e03e78e94685f40b6cead009c4550370b22a16184aa8822d48
    tpSize: 0.001
    tpTriggerPrice: 70000
    tpTriggerPriceType: INDEX
   */
  placeOrder = async (opts: PlaceOrderOpts) => {
    // Resolved before SDK init and signing: the ID is hashed into the signed nonce.
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);
    if (
      !Number.isFinite(amount) ||
      amount <= 0 ||
      amount < market.limits.amount.min ||
      !Number.isFinite(maxSize) ||
      maxSize <= 0 ||
      pAmount <= 0
    ) {
      throw new Error('ApeX Omni order size or market lot limits are invalid');
    }

    const price = opts.price ? adjust(opts.price, pPrice) : null;
    const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
    const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;

    if (amount > maxSize && (stopLoss || takeProfit)) {
      // ApeX documents parent + TP/SL creation only within one /v3/order payload.
      // Its SDK-only batch route has no documented all-or-none contract.
      const message = 'ApeX Omni cannot atomically attach TP/SL to split orders';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    let sizes = [amount];
    if (amount > maxSize) {
      const ticks = Math.round(amount / pAmount);
      const maxTicks = Math.floor(maxSize / pAmount);
      const count = Math.ceil(ticks / maxTicks);
      const baseTicks = Math.floor(ticks / count);
      if (
        !Number.isSafeInteger(ticks) ||
        !Number.isFinite(count) ||
        count > 1000 ||
        baseTicks < Math.max(1, Math.ceil(market.limits.amount.min / pAmount))
      ) {
        throw new Error('ApeX Omni order cannot be split within market lot limits');
      }
      const remainder = ticks - baseTicks * count;
      sizes = Array.from({ length: count }, (_, index) =>
        adjust((baseTicks + Number(index < remainder)) * pAmount, pAmount),
      );
    }

    await initSdk();

    const signer = sdk.newRpcSignerWithProvider({});
    await signer.initZklinkSigner(this.options.omniKeySeed);

    const timeInForce = inverseObj(ORDER_TIME_IN_FORCE)[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
    const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;
    const ticker = this.store.tickersBySymbol[opts.symbol]!;

    let type = opts.type;

    // Calculate execution price for stop/TP orders
    let executionPrice: string;
    if (opts.type === OrderType.Limit) {
      executionPrice = `${price}`;
    } else if (opts.type === OrderType.Market) {
      executionPrice = `${adjust(ticker.mark * (opts.side === OrderSide.Buy ? 1.02 : 0.98), pPrice)}`;
    } else if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) {
      // For stop/TP orders, calculate execution price based on trigger price with slippage
      const effectiveTriggerPrice = triggerPrice || price || ticker.mark;
      executionPrice = `${calculateTpSlPrice({
        side: inverseObj(ORDER_SIDE)[opts.side],
        marketPrice: effectiveTriggerPrice,
        priceStep: pPrice,
      })}`;
    } else {
      executionPrice = `${pPrice}`;
    }

    const mappedOrderType = inverseObj(ORDER_TYPE)[type];
    const mappedSide = inverseObj(ORDER_SIDE)[opts.side];

    // For TP/SL orders, triggerPrice comes from either opts.triggerPrice or opts.price
    // opts.price is the trigger price for stop orders in our system
    const calculatedTriggerPrice =
      opts.type === OrderType.TakeProfit || opts.type === OrderType.StopLoss
        ? `${triggerPrice || price}` // Use triggerPrice if available, otherwise use price as trigger
        : undefined; // Regular orders don't have triggerPrice

    const req = omitUndefined({
      symbol: opts.symbol.includes('-') ? opts.symbol : market.id,
      side: mappedSide,
      type: mappedOrderType,
      size: `${amount}`,
      price: executionPrice,
      reduceOnly: opts.reduceOnly || false,
      triggerPrice: calculatedTriggerPrice,
      triggerPriceType:
        opts.type === OrderType.StopLoss
          ? opts.slTriggerType === OrderStopTriggerType.LastPrice
            ? 'MARKET'
            : 'INDEX'
          : opts.type === OrderType.TakeProfit
            ? opts.tpTriggerType === OrderStopTriggerType.IndexPrice
              ? 'INDEX'
              : 'MARKET'
            : undefined,
      timeInForce,
      brokerId: BROKER_ID,
    });

    this.log(`Order request: ${JSON.stringify(req)}`, LogSeverity.Info);

    const payloads = sizes.map((size) => ({ ...req, size: `${size}` }));

    const responses = await mapSeries(payloads, async (p, index) => {
      const clientOrderId = callerClientOrderId ?? generateApeXRandomClientId(String(this.store.zkLinkConfig?.id));
      const expirationIsoTimestamp = (virtualClock.getCurrentTimeMs() + 30 * 24 * 60 * 60 * 1000) as any;

      // if (!this.store.starkConfig) {
      //   this.emitAndLogError('Stark key pair not found');
      //   throw new Error('Stark key pair not found');
      // }

      let l2PairId = 0;

      for (const symbol of this.store.zkLinkConfig?.contractConfig.perpetualContract) {
        if (symbol.symbol === p.symbol) {
          l2PairId = Number(symbol.l2PairId);
        }
      }

      const takerFeeRate = this.store.zkLinkConfig?.contractAccount.takerFeeRate;

      const limitFee = feeHelper(
        new BigNumber(p.price || 0)
          .multipliedBy(takerFeeRate || '0')
          .multipliedBy(p.size || 0)
          .toNumber(),
        6,
        'ceil',
      );

      const nonce = new BigNumber(toHex(hashClientOrderId(clientOrderId)), 16);

      const orderToSign: SignContractPayload = {
        direction: opts.side === OrderSide.Buy,
        feeRates: [
          Math.ceil(parseFloat(this.store.zkLinkConfig?.contractAccount.makerFeeRate) * 1e4),
          Math.ceil(parseFloat(this.store.zkLinkConfig?.contractAccount.takerFeeRate) * 1e4),
        ],
        nonce: nonce.mod(UINT32_MAX).toNumber(),
        pairId: l2PairId,
        price: new BigNumber(p.price || 0).multipliedBy(1e18).toFixed(0, BigNumber.ROUND_DOWN),
        size: new BigNumber(p.size || 0).multipliedBy(1e18).toFixed(0, BigNumber.ROUND_DOWN),
        slotId: nonce.mod(UINT64_MAX).div(UINT32_MAX).toNumber(),
        accountId: new BigNumber(this.store.zkLinkConfig?.id || 0, 10).mod(UINT32_MAX).toNumber(),
        subAccountId: 0,
        hasSubsidy: false,
      };

      const expiration =
        addOrderExpirationBufferHours(isoTimestampToEpochHours(expirationIsoTimestamp)) * 60 * 60 * 1000;

      const params: any = {
        ...p,
        clientId: clientOrderId,
        signature: await this.signOrder(signer, orderToSign),
        limitFee,
        expiration,
        clientOrderId,
      };

      const includeBrackets = index === 0;

      if (includeBrackets && (takeProfit || stopLoss)) {
        params.isOpenTpslOrder = true;
      }

      if (includeBrackets && takeProfit) {
        params.isSetOpenTp = true;

        const clientOrderId = generateApeXRandomClientId(String(this.store.zkLinkConfig?.id));
        const tpNonce = new BigNumber(toHex(hashClientOrderId(clientOrderId)), 16);

        const side = (`${p.side}` === 'BUY' ? 'SELL' : 'BUY') as OrderWithClientId['side'];

        const tpPrice = calculateTpSlPrice({ side, marketPrice: takeProfit, priceStep: pPrice });

        const limitFee = feeHelper(
          new BigNumber(`${tpPrice}`)
            .multipliedBy(takerFeeRate || '0')
            .multipliedBy(p.size || 0)
            .toNumber(),
          6,
          'ceil',
        );

        const orderToSign: SignContractPayload = {
          direction: opts.side !== OrderSide.Buy,
          feeRates: [
            Math.ceil(parseFloat(this.store.zkLinkConfig?.contractAccount.makerFeeRate) * 1e4),
            Math.ceil(parseFloat(this.store.zkLinkConfig?.contractAccount.takerFeeRate) * 1e4),
          ],
          nonce: tpNonce.mod(UINT32_MAX).toNumber(),
          pairId: l2PairId,
          price: new BigNumber(tpPrice || 0).multipliedBy(1e18).toFixed(0, BigNumber.ROUND_DOWN),
          size: new BigNumber(p.size || 0).multipliedBy(1e18).toFixed(0, BigNumber.ROUND_DOWN),
          slotId: tpNonce.mod(UINT64_MAX).div(UINT32_MAX).toNumber(),
          accountId: new BigNumber(this.store.zkLinkConfig?.id || 0, 10).mod(UINT32_MAX).toNumber(),
          subAccountId: 0,
          hasSubsidy: false,
        };

        params.tpClientOrderId = clientOrderId;
        params.tpExpiration = expiration;
        params.tpLimitFee = limitFee;
        params.tpPrice = `${tpPrice}`;
        params.tpSide = side;
        params.tpSignature = await this.signOrder(signer, orderToSign);
        params.tpSize = `${p.size}`;
        params.tpTriggerPrice = takeProfit;
        params.tpTriggerPriceType = opts.tpTriggerType === OrderStopTriggerType.IndexPrice ? 'INDEX' : 'MARKET';
      }

      if (includeBrackets && stopLoss) {
        params.isSetOpenSl = true;

        const clientOrderId = generateApeXRandomClientId(String(this.store.zkLinkConfig?.id));
        const slNonce = new BigNumber(toHex(hashClientOrderId(clientOrderId)), 16);

        const side = (`${p.side}` === 'BUY' ? 'SELL' : 'BUY') as OrderWithClientId['side'];

        const slPrice = calculateTpSlPrice({ side, marketPrice: stopLoss, priceStep: pPrice });

        const limitFee = feeHelper(
          new BigNumber(`${slPrice}`)
            .multipliedBy(takerFeeRate || '0')
            .multipliedBy(p.size || 0)
            .toNumber(),
          6,
          'ceil',
        );

        const orderToSign: SignContractPayload = {
          direction: opts.side !== OrderSide.Buy,
          feeRates: [
            Math.ceil(parseFloat(this.store.zkLinkConfig?.contractAccount.makerFeeRate) * 1e4),
            Math.ceil(parseFloat(this.store.zkLinkConfig?.contractAccount.takerFeeRate) * 1e4),
          ],
          nonce: slNonce.mod(UINT32_MAX).toNumber(),
          pairId: l2PairId,
          price: new BigNumber(slPrice || 0).multipliedBy(1e18).toFixed(0, BigNumber.ROUND_DOWN),
          size: new BigNumber(p.size || 0).multipliedBy(1e18).toFixed(0, BigNumber.ROUND_DOWN),
          slotId: slNonce.mod(UINT64_MAX).div(UINT32_MAX).toNumber(),
          accountId: new BigNumber(this.store.zkLinkConfig?.id || 0, 10).mod(UINT32_MAX).toNumber(),
          subAccountId: 0,
          hasSubsidy: false,
        };

        params.slClientOrderId = clientOrderId;
        params.slExpiration = expiration;
        params.slLimitFee = limitFee;
        params.slPrice = `${slPrice}`;
        params.slSide = side;
        params.slSignature = await this.signOrder(signer, orderToSign);
        params.slSize = `${p.size}`;
        params.slTriggerPrice = stopLoss;
        params.slTriggerPriceType = opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'MARKET' : 'INDEX';
      }

      const stringifiedParams = qs.stringify(params, {
        filter(prefix, value) {
          if (value) {
            return value;
          }
          return;
        },
        sort(a, b) {
          return a.localeCompare(b);
        },
      });

      const { data } = await this.unlimitedXHR.post(ENDPOINTS.CREATE_ORDER, stringifiedParams, {
        headers: {
          'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
        },
      });
      return data;
    });

    const fullfilled = responses.filter((r) => r !== undefined);

    const orderIds = fullfilled.map((resp) => resp.data.id);

    // Trigger order fetch via websocket handler
    // This will be throttled to avoid excessive API calls
    this.privateWebsocket.throttleFetchOrders();

    return orderIds;
  };

  signOrder = async (signer: sdk.JsonRpcSigner, payload: any, _prefix = '') => {
    const signature = await signContract(signer, payload);
    return signature.tx.signature.signature;
  };

  // Cancellation removes and blacklists the parent before its reply arrives.
  private amendmentObservers = new Map<
    string,
    Set<(quantity: number, terminal: boolean, fill: boolean, timestamp?: number) => void>
  >();

  noteAmendOrderProgress = (id: string, filled: number, terminal: boolean, updatedAt?: unknown) => {
    if (!id || !Number.isFinite(filled) || filled < 0) return;
    const timestamp = this.normalizeExecutionTimestampToMilliseconds(updatedAt) ?? virtualClock.getCurrentTimeMs();
    this.amendmentObservers.get(id)?.forEach((observe) => observe(filled, terminal, false, timestamp));
  };

  noteAmendFillProgress = (execution: Execution, amount: number) => {
    const id = execution.orderId;
    const timestamp = this.executionTimestamp(execution);
    if (!id || !Number.isFinite(amount) || amount <= 0) return;
    if (!Number.isFinite(timestamp)) return;
    this.amendmentObservers.get(id)?.forEach((observe) => observe(amount, false, true, timestamp));
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const explicitAmount = 'amount' in update ? update.amount : undefined;
    const explicitPrice = 'price' in update ? update.price : undefined;
    if (explicitAmount === undefined && explicitPrice === undefined) {
      throw new Error('ApeX Omni amendment requires a price or amount');
    }
    if (
      [explicitAmount, explicitPrice].some((value) => value !== undefined && (!Number.isFinite(value) || value <= 0))
    ) {
      throw new Error('ApeX Omni amendment values must be finite and positive');
    }
    const baseOrderId = order.id.replace(/__(take_profit|stop_loss)$/, '');
    const baseOrder = this.store.orders.find((stored) => stored.id === baseOrderId);
    if (!baseOrder) throw new Error(`Base order ${baseOrderId} not found`);
    if (![OrderStatus.Open, OrderStatus.Creating].includes(baseOrder.status) || baseOrder.remaining <= 0) {
      throw new Error('ApeX Omni cannot amend a terminal order');
    }
    const stopLossOrder = this.store.orders.find((stored) => stored.id === `${baseOrderId}__stop_loss`);
    const takeProfitOrder = this.store.orders.find((stored) => stored.id === `${baseOrderId}__take_profit`);
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: baseOrder.symbol,
      type: baseOrder.type,
      side: baseOrder.side,
      price: baseOrder.price,
      amount: explicitAmount ?? baseOrder.remaining,
      reduceOnly: baseOrder.reduceOnly || false,
      ...(stopLossOrder && { stopLoss: stopLossOrder.price, slTriggerType: stopLossOrder.slTriggerType }),
      ...(takeProfitOrder && { takeProfit: takeProfitOrder.price, tpTriggerType: takeProfitOrder.tpTriggerType }),
    };
    if (!Number.isFinite(newOrder.amount) || newOrder.amount <= 0) {
      throw new Error('ApeX Omni replacement amount must be finite and positive');
    }
    if ('tpTriggerType' in baseOrder) newOrder.tpTriggerType = baseOrder.tpTriggerType;
    if ('slTriggerType' in baseOrder) newOrder.slTriggerType = baseOrder.slTriggerType;
    if (explicitPrice !== undefined) {
      if (order.id.endsWith('__take_profit')) newOrder.takeProfit = explicitPrice;
      else if (order.id.endsWith('__stop_loss')) newOrder.stopLoss = explicitPrice;
      else newOrder.price = explicitPrice;
    }

    const baselineFilled = baseOrder.filled;
    const baselineRemaining = baseOrder.remaining;
    const totalAmount = baseOrder.amount;
    const started = virtualClock.getCurrentTimeMs();
    let filled = baselineFilled;
    let fillTotal = 0;
    const fills: Array<[number, number]> = [];
    let reported = baselineFilled;
    let reportedAt = started;
    let overflow = false;
    let terminal = false;
    const observe = (quantity: number, closed: boolean, execution: boolean, timestamp?: number) => {
      if (execution && (timestamp === undefined || timestamp < started)) return;
      if (execution) {
        if (fills.length === 4096) {
          overflow = true;
          return;
        }
        fillTotal += quantity;
        fills.push([timestamp!, quantity]);
      } else {
        if (quantity > reported && timestamp !== undefined) {
          reported = quantity;
          reportedAt = timestamp;
        }
        filled = Math.max(filled, quantity);
      }
      const afterReport = fills.reduce((total, [time, amount]) => total + (time > reportedAt ? amount : 0), 0);
      filled = Math.max(filled, baselineFilled + fillTotal, reported + afterReport);
      terminal ||= closed;
    };
    const observers = this.amendmentObservers.get(baseOrderId) ?? new Set();
    observers.add(observe);
    this.amendmentObservers.set(baseOrderId, observers);
    try {
      await this.cancelOrders([baseOrder]);
      const latest = this.store.orders.find((stored) => stored.id === baseOrderId);
      if (latest) {
        filled = Math.max(filled, latest.filled);
        terminal ||= latest.status === OrderStatus.Closed || latest.remaining <= 0;
      }
      if (overflow) throw new Error('ApeX Omni amendment exceeded the observed fill limit');
      if (terminal || filled >= totalAmount) return [];
      if (explicitAmount === undefined) newOrder.amount = Math.max(0, baselineRemaining - (filled - baselineFilled));
      if (newOrder.amount <= 0) return [];
      return await this.placeOrder(newOrder);
    } finally {
      observers.delete(observe);
      if (observers.size === 0) this.amendmentObservers.delete(baseOrderId);
    }
  };

  cancelOrders = async (orders: Order[]) => {
    const ordersByCategory = [orders];
    await forEachSeries(Object.entries(ordersByCategory), async ([category, orders]) => {
      await this.cancelOrdersHelper(orders, category);
    });
  };

  cancelOrdersHelper = async (orders: Order[], _category: string) => {
    // For TP/SL orders with composite IDs, we need to extract the base order ID
    // and only cancel the base order once (which will cancel all associated TP/SL)
    const baseOrderIds = new Set<string>();
    const ordersToCancel: string[] = [];
    const allOrderIdsToRemove: string[] = [];

    orders.forEach((order) => {
      if (order.id.includes('__stop_loss') || order.id.includes('__take_profit')) {
        // Extract base order ID from composite ID
        const baseOrderId = order.id.split('__')[0];
        if (!baseOrderIds.has(baseOrderId)) {
          baseOrderIds.add(baseOrderId);
          ordersToCancel.push(baseOrderId);
        }
        allOrderIdsToRemove.push(order.id);
      } else {
        // Regular order, cancel directly
        ordersToCancel.push(order.id);
        allOrderIdsToRemove.push(order.id);
        // Also track associated TP/SL orders that should be removed
        allOrderIdsToRemove.push(`${order.id}__stop_loss`);
        allOrderIdsToRemove.push(`${order.id}__take_profit`);
      }
    });

    // Add to blacklist BEFORE making API calls to prevent race conditions
    const now = Date.now();
    ordersToCancel.forEach((orderId) => {
      this.recentlyCancelledOrders.set(orderId, now);
    });

    // Immediately remove all cancelled orders from the store
    this.store.removeOrders(allOrderIdsToRemove.map((id) => ({ id })));

    // group into lots of 10
    const lots = chunk(ordersToCancel, 10);

    await forEachSeries(lots, async (orderIds, i) => {
      if (i > 0) await sleep(1000);

      const errors: Array<string | undefined | null> = [];
      await forEachSeries(orderIds, async (orderId) => {
        await this.unlimitedXHR.post(
          ENDPOINTS.CANCEL_ORDER,
          {
            id: orderId,
          },
          {
            headers: {
              'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
            },
          },
        );
      });
      if (errors.length > 0) {
        this.emitAndLogError(errors.join(', '));
        throw new Error(errors.join(', '));
      }
    });
  };

  cancelSymbolOrders = async (symbol: string, cancelTpSl = true) => {
    const tpOrSLOrders = cancelTpSl
      ? []
      : this.store.orders.filter((o) => o.symbol === symbol && o.type !== OrderType.Limit);

    const market = this.store.marketsBySymbol[symbol];

    await this.unlimitedXHR.post(
      ENDPOINTS.CANCEL_ORDERS,
      {
        symbol: symbol.includes('-') ? symbol : market!.id,
      },
      {
        headers: {
          'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
        },
      },
    );

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
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    let foundPositions = this.store.positions.filter((p) => p.symbol === symbol);
    if (foundPositions.length === 0) {
      const fetchedPositions = await this.fetchPositions();
      this.store.addOrUpdatePositions(fetchedPositions);
      foundPositions = fetchedPositions.filter((p) => p.symbol === symbol);
    }
    const position = foundPositions[0];
    if (!position) {
      throw new Error(`Position state for ${symbol} is unavailable; cannot set leverage`);
    }

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      await this.unlimitedXHR.post(
        ENDPOINTS.SET_LEVERAGE,
        {
          symbol: market!.id,
          token: symbol.endsWith('USDT') ? 'USDT' : 'USDC',
          initialMarginRate: (1 / +leverage).toFixed(5),
        },
        {
          headers: {
            'Content-Type': 'multipart/form-data',
          },
        },
      );

      // const positions = await this.fetchPositions();
      // this.store.addOrUpdatePositions(positions);
    }
  };

  mapPosition(p: Record<string, any>) {
    if (typeof p.symbol !== 'string' || !p.symbol.trim()) {
      throw new Error('ApeX position snapshot carried an invalid symbol');
    }
    const entryPrice = parseFloat(v(p, 'avgPrice') || v(p, 'entryPrice') || 0);
    const contracts = parseFloat(p.size ?? 0);
    // const side =
    //   p.positionIdx === 1
    //     ? PositionSide.Long
    //     : p.positionIdx === 2
    //       ? PositionSide.Short
    //       : POSITION_SIDE[p.side] || PositionSide.Long;
    const side = p.side === 'SHORT' ? PositionSide.Short : PositionSide.Long;

    let unrealizedPnl = parseFloat(v(p, 'unrealisedPnl') || 0);
    // if (!p.symbol.endsWith('USDT')) {
    //   const market = this.store.markets.find((m) => m.id === p.symbol);
    //   const ticker = this.store.tickers.find((t) => t.id === p.symbol);
    //   if (market && market.quote === 'USDC' && ticker) {
    //     const direction = side === PositionSide.Long ? 1 : -1;
    //     unrealizedPnl = direction * (ticker.mark * contracts - entryPrice * contracts);
    //   }
    // }

    // const positionMode =
    //   p.positionIdx === undefined ? undefined : p.positionIdx === 0 ? PositionMode.OneWay : PositionMode.Hedged;
    // const marginType =
    //   p.tradeMode === undefined ? undefined : p.tradeMode === 1 ? MarginType.Isolated : MarginType.Cross;
    const market = this.store.marketsById[p.symbol];

    // Skip this position if market is not found
    if (!market) {
      this.log(`Market not found for position symbol: ${p.symbol}`, LogSeverity.Warning);
      return null;
    }
    if (p.size === undefined || p.size === null || !['LONG', 'SHORT'].includes(p.side)) {
      throw new Error(`ApeX position snapshot carried invalid size or side for ${market.symbol}`);
    }
    const customImr = v(p, 'customImr');
    const customInitialMarginRate = v(p, 'customInitialMarginRate');
    for (const value of [
      p.size,
      v(p, 'avgPrice'),
      v(p, 'entryPrice'),
      v(p, 'unrealisedPnl'),
      v(p, 'liqPrice'),
      v(p, 'curRealisedPnl'),
      v(p, 'realizedPnl'),
      customImr,
      customInitialMarginRate,
    ]) {
      if (
        value !== undefined &&
        value !== null &&
        ((typeof value !== 'string' && typeof value !== 'number') ||
          `${value}`.trim() === '' ||
          !Number.isFinite(Number(value)))
      ) {
        throw new Error(`ApeX position snapshot carried invalid quantities for ${market.symbol}`);
      }
    }

    const id = createPositionId(market.symbol, side, PositionMode.Hedged);

    const position: Position = {
      id,
      symbol: market.symbol,
      side,
      entryPrice,
      notional: +contracts * +entryPrice,
      leverage: Math.round(
        +customImr > 0
          ? 1 / +customImr
          : +customInitialMarginRate > 0
            ? 1 / +customInitialMarginRate
            : this.store.positions.find((p) => p.id === id)?.leverage
              ? this.store.positions.find((p) => p.id === id)!.leverage
              : 1 / market?.info?.defaultInitialMarginRate || 0,
      ),
      unrealizedPnl,
      realizedPnl: parseFloat(v(p, 'curRealisedPnl') ?? v(p, 'realizedPnl') ?? 0),
      contracts,
      liquidationPrice: parseFloat(v(p, 'liqPrice') || 0),
      positionMode: PositionMode.Hedged,
      marginType: MarginType.Cross,
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    if (
      !Number.isFinite(position.contracts) ||
      position.contracts < 0 ||
      !Number.isFinite(position.entryPrice) ||
      position.entryPrice < 0 ||
      !Number.isFinite(position.notional) ||
      !Number.isFinite(position.leverage) ||
      !Number.isFinite(position.unrealizedPnl) ||
      !Number.isFinite(position.realizedPnl) ||
      !Number.isFinite(position.liquidationPrice)
    ) {
      throw new Error(`ApeX position snapshot carried invalid quantities for ${market.symbol}`);
    }

    return position;
  }

  // {
  //   "id": "612003166120051509",
  //   "clientId": "2214491449240652",
  //   "clientOrderId": "2214491449240652",
  //   "accountId": "611779989544632629",
  //   "symbol": "BTC-USDC",
  //   "side": "SELL",
  //   "price": "64000.0",
  //   "averagePrice": "",
  //   "limitFee": "0.032000",
  //   "fee": "",
  //   "liquidateFee": "",
  //   "triggerPrice": "0.0",
  //   "size": "0.001",
  //   "type": "LIMIT",
  //   "createdAt": 1723749725272,
  //   "updatedTime": 1723749725282,
  //   "expiresAt": 1726344000000,
  //   "status": "OPEN",
  //   "timeInForce": "GOOD_TIL_CANCEL",
  //   "reduceOnly": true,
  //   "isPositionTpsl": false,
  //   "orderId": "612003166120051509",
  //   "exitType": "",
  //   "latestMatchFillPrice": "0.0",
  //   "cumMatchFillSize": "0.000",
  //   "cumMatchFillValue": "0.0000",
  //   "cumMatchFillFee": "0.000000",
  //   "cumSuccessFillSize": "0.000",
  //   "cumSuccessFillValue": "0.0000",
  //   "cumSuccessFillFee": "0.000000",
  //   "triggerPriceType": "MARKET",
  //   "isOpenTpslOrder": false,
  //   "isSetOpenTp": false,
  //   "isSetOpenSl": false,
  //   "openTpParam": {},
  //   "openSlParam": {}
  // }

  // TP/SL limit order
  // "cumSuccessLiquidateFee": "0.000000",
  // "symbol": "BTC-USDC",
  // "openSlParams": {
  //   "triggerSize": "0.001",
  //   "triggerPrice": "52000",
  //   "triggerPriceType": "INDEX"
  // },
  // "cumSuccessFillFee": "0.000000",
  // "type": "LIMIT",
  // "isPositionTpsl": false,
  // "isDeleverage": false,
  // "createdAt": 1723965826265,
  // "isSetOpenTp": true,
  // "price": "54600.0",
  // "cumSuccessFillValue": "0.0000",
  // "id": "612909559379395381",
  // "cancelReason": "UNKNOWN_ORDER_CANCEL_REASON",
  // "timeInForce": "POST_ONLY",
  // "updatedAt": 1723965826274,
  // "limitFee": "0.027300",
  // "side": "BUY",
  // "clientId": "9685076048960317",
  // "triggerPrice": "0.0",
  // "triggerPriceType": "MARKET",
  // "expiresAt": 1726560000000,
  // "openTpParams": {
  //   "triggerSize": "0.001",
  //   "triggerPrice": "64000",
  //   "triggerPriceType": "INDEX"
  // },
  // "cumSuccessFillSize": "0.000",
  // "accountId": "611779989544632629",
  // "size": "0.001",
  // "reduceOnly": false,
  // "isSetOpenSl": true,
  // "isLiquidate": false,
  // "remainingSize": "0.001",
  // "status": "OPEN"
  mapOrder(o: Record<string, any>) {
    if (
      typeof o.symbol !== 'string' ||
      !o.symbol.trim() ||
      !['string', 'number'].includes(typeof o.id) ||
      !`${o.id}`.trim() ||
      (typeof o.id === 'number' && !Number.isFinite(o.id))
    ) {
      throw new Error('ApeX order snapshot carried an invalid symbol or ID');
    }
    // const isStop = o.stopOrderType !== 'UNKNOWN' && o.stopOrderType !== '';
    //
    // const oPrice = isStop ? v(o, 'triggerPrice') : o.price;
    // const oType = isStop ? v(o, 'stopOrderType') : v(o, 'orderType');

    const type = ORDER_TYPE[v(o, 'type')];
    const timeInForce = o.postOnly ? OrderTimeInForce.PostOnly : ORDER_TIME_IN_FORCE[o.timeInForce];

    const market = this.store.marketsById[o.symbol];

    if (!market) {
      console.warn('[APEX OMNI] Market not found for order:', o.symbol);
      return [];
    }

    const orders: Order[] = [
      {
        id: o.id,
        status: ORDER_STATUS[v(o, 'status')],
        symbol: market.symbol,
        type,
        side: ORDER_SIDE[o.side?.toUpperCase()],
        price: parseFloat(v(o, 'triggerPrice') && +o.triggerPrice > 0 ? v(o, 'triggerPrice') : v(o, 'price')),
        //amount: (oType === 'StopLoss' || oType === 'TakeProfit') && o.tpslMode === 'Full' ? 0 : parseFloat(o.qty ?? 0)
        amount: parseFloat(o.size ?? 0),
        filled: parseFloat(v(o, 'cumSuccessFillSize') ?? 0),
        reduceOnly: v(o, 'reduceOnly') || false,
        remaining: subtract(o.size ?? 0, v(o, 'cumSuccessFillSize') ?? 0),
        clientOrderId: o.clientOrderId || o.clientId || undefined,
        info: o,
        timeInForce,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        ...(type === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerPriceType')] }),
        ...(type === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerPriceType')] }),
      },
    ];

    const order = orders[0];
    if (
      !order.id ||
      o.size === undefined ||
      o.size === null ||
      !order.status ||
      !order.type ||
      !order.side ||
      !Number.isFinite(order.price) ||
      order.price < 0 ||
      !Number.isFinite(order.amount) ||
      order.amount < 0 ||
      !Number.isFinite(order.filled) ||
      order.filled < 0 ||
      order.filled > order.amount
    ) {
      throw new Error(`ApeX order snapshot carried invalid quantities for ${market.symbol}`);
    }

    // "openSlParams": {
    //   "triggerSize": "0.001",
    //   "triggerPrice": "52000",
    //   "triggerPriceType": "INDEX"
    // },

    // API returns both singular and plural versions, check both
    const slPart = v(o, 'openSlParams') || v(o, 'openSlParam');
    const tpPart = v(o, 'openTpParams') || v(o, 'openTpParam');

    const sl = parseFloat(v(slPart, 'triggerPrice'));
    const tp = parseFloat(v(tpPart, 'triggerPrice'));
    for (const part of [slPart, tpPart]) {
      const rawPrice = v(part, 'triggerPrice');
      if (rawPrice !== undefined && rawPrice !== null && rawPrice !== '' && !Number.isFinite(Number(rawPrice))) {
        throw new Error(`ApeX order snapshot carried an invalid bracket price for ${market.symbol}`);
      }
    }

    const inverseSide = orders[0].side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;

    if (sl > 0) {
      orders.push({
        ...orders[0],
        id: `${o.id}__stop_loss`,
        type: OrderType.StopLoss,
        side: inverseSide,
        price: sl,
        filled: 0,
        remaining: orders[0].amount,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        slTriggerType: ORDER_TRIGGER_TYPE[v(slPart, 'triggerPriceType')],
      });
    }

    if (tp > 0) {
      orders.push({
        ...orders[0],
        id: `${o.id}__take_profit`,
        type: OrderType.TakeProfit,
        side: inverseSide,
        price: tp,
        filled: 0,
        remaining: orders[0].amount,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        tpTriggerType: ORDER_TRIGGER_TYPE[v(tpPart, 'triggerPriceType')],
      });
    }

    return orders;
  }
}

function calculateTpSlPrice({
  side,
  marketPrice,
  priceStep,
}: {
  side: string;
  marketPrice: number;
  priceStep: number;
}) {
  // For STOP_MARKET and TAKE_PROFIT_MARKET orders, the execution price must be
  // slightly worse than the trigger price to allow for minimal slippage:
  // - BUY orders: set to 0.5% above trigger price
  // - SELL orders: set to 0.5% below trigger price
  // Using minimal slippage to stay close to trigger price
  const slippageMultiplier = side === 'BUY' ? 1.005 : 0.995;
  const executionPrice = marketPrice * slippageMultiplier;
  // Round to price step precision using adjust to avoid floating point errors
  return adjust(executionPrice, priceStep);
}
