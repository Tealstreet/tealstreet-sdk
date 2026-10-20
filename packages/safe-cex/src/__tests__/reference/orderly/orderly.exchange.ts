import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import cloneDeep from 'lodash/cloneDeep';
import sumBy from 'lodash/sumBy';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';
import { forEachSeries, mapSeries } from 'p-iteration';

import {
  Candle,
  ExchangeName,
  ExchangeOptions,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, subtract } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './orderly.api';
import {
  ACCOUNT_RESPONSE,
  EDIT_ORDER_REQUEST,
  EDIT_ORDER_RESPONSE,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TYPE,
  PLACE_ORDER_RESPONSE,
} from './orderly.types';
import { normalizeSymbol, reverseSymbol } from './orderly.utils';
import { OrderlyPrivateWebSocket } from './orderly.ws-private';
import { OrderlyPublicWebsocket } from './orderly.ws-public';

/**
 * Classify an Orderly account-validation failure as transient vs. a definitive
 * auth rejection.
 *
 * `start()` runs on every (re)connect, so a reconnect-time network blip,
 * timeout, rate-limit, or 5xx on the single `GET /v1/client/info` validation
 * call must NOT be treated as "API key expired". Doing so emits `deactivate`,
 * which the web app turns into `account.active = false` (exchangeAccountController
 * `deactivate-status`) and then `CleanupComponentWorker` force-switches the user
 * off their selected account — for an account that was merely reconnecting.
 *
 * Only a real 4xx auth rejection should deactivate. No HTTP response at all
 * (network error / CORS / proxy / tunnel drop) is transient by definition.
 */
const isTransientOrderlyValidationError = (err: any): boolean => {
  const status: number | undefined = err?.response?.status;
  if (status == null) return true; // network error / timeout / CORS / proxy / tunnel drop
  if (status === 408 || status === 429) return true; // request timeout / rate limited
  if (status >= 500) return true; // exchange-side error
  return false; // 4xx (401/403/…) — credentials or request rejected
};

const EXPECTED_ORDERLY_ORDER_REJECTION_PATTERNS = [
  /The order value should be greater or equal to [\d.]+/i,
  /The margin will be insufficient after/i,
  /The TP_SL side should be opposite to position side/i,
];

const isExpectedOrderlyOrderRejection = (message?: string): boolean =>
  typeof message === 'string' && EXPECTED_ORDERLY_ORDER_REJECTION_PATTERNS.some((pattern) => pattern.test(message));

const getOrderlyOrderErrorLogOptions = (message?: string): Record<string, boolean> | undefined =>
  isExpectedOrderlyOrderRejection(message) ? { suppressConsoleLog: true } : undefined;

export class OrderlyExchange extends BaseExchange {
  // Orderly: up to 36 characters, and a hyphen may not be the first character.
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Orderly', /^(?!-).{1,36}$/);

  xhr: Axios;

  publicWebsocket: OrderlyPublicWebsocket;
  privateWebsocket: OrderlyPrivateWebSocket;

  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;
  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;

  orderlyExchangeName = 'orderly';

  // Orderly stores leverage per account, not per position
  // as workaround we store the account leverage here when
  // we call the ACCOUNT endpoint, and copy this value into positions
  private accountLeverage = 1;
  private _accountPositionMode: PositionMode | null = null;
  private marginTypeBySymbol = new Map<string, MarginType>();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    // Ensure key and secret have the "ed25519:" prefix
    const normalizedOpts = { ...opts };
    if (normalizedOpts.key && !normalizedOpts.key.startsWith('ed25519:')) {
      normalizedOpts.key = `ed25519:${normalizedOpts.key}`;
    }
    if (normalizedOpts.secret && !normalizedOpts.secret.startsWith('ed25519:')) {
      normalizedOpts.secret = `ed25519:${normalizedOpts.secret}`;
    }

    super(exchangeName, normalizedOpts, store);

    this.xhr = rateLimit(createAPI(normalizedOpts, this), { maxRPS: 10 });

    this.publicWebsocket = new OrderlyPublicWebsocket(this);
    this.privateWebsocket = new OrderlyPrivateWebSocket(this);
  }

  dispose = () => {
    this.isDisposed = true;
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  async getPositionMode(): Promise<PositionMode> {
    if (!this._accountPositionMode) {
      await this.xhr.get<{ data: ACCOUNT_RESPONSE }>(ENDPOINTS.ACCOUNT);
      this._accountPositionMode = PositionMode.OneWay;
    }
    return this._accountPositionMode;
  }

  getAccount = async () => {
    const {
      data: { data },
    } = await this.xhr.get<{ data: ACCOUNT_RESPONSE }>(ENDPOINTS.ACCOUNT);

    return {
      userId: data.account_id,
      affiliateId: data.account_id || null,
    };
  };

  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.ACCOUNT);
      return { error: '' };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
    const publicDataOnly = this.isPublicDataOnly;

    // Validate account at the start (skip for public-only mode).
    // NOTE: validated inline (not via validateAccount(), which collapses the
    // error to a string) so we can classify transient vs. auth failures and
    // only deactivate on the latter.
    if (!publicDataOnly) {
      try {
        await this.xhr.get(ENDPOINTS.ACCOUNT);
      } catch (err: any) {
        // Surface the underlying error either way (honors the app-side
        // showErrorsWhileReconnecting gate).
        this.emitAndLogError(err?.response?.data?.message || err?.message);

        if (isTransientOrderlyValidationError(err)) {
          // Network / timeout / rate-limit / 5xx during a (re)connect: the key
          // is almost certainly fine and the account is just reconnecting.
          // Throw so the worker handles this like any other exchange's
          // transient start() failure (logs it, leaves the account ACTIVE).
          // Do NOT emit `deactivate` — that disables the account and
          // force-switches the user off their selected account on a transient
          // reconnect.
          throw new Error(
            `Orderly account validation failed transiently, will retry on next reconnect: ${
              err?.response?.data?.msg || err?.message || 'unknown error'
            }`,
          );
        }

        // Definitive auth rejection (expired / revoked / wrong key) — disable
        // the account so we stop hammering the API and tell the user.
        this.emitAndLogError('API key expired');
        this.emitter.emit('deactivate');
        return;
      }
    }

    const markets = await this.fetchMarkets();
    if (this.isDisposed) return;

    this.store.update({
      markets,
      loaded: { ...this.store.loaded, markets: true },
    });

    const tickers = await this.fetchTickers();
    if (this.isDisposed) return;

    if (!publicDataOnly) {
      try {
        await this.fetchAndUpdateMarginTypes();
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.message || err?.message || 'Orderly margin-mode fetch failed');
      }
      void this.fetchAndUpdateAccountLeverage().catch((err: any) => {
        this.emitAndLogError(err?.response?.data?.message || err?.message || 'Orderly account leverage fetch failed');
      });
    }

    this.log(`Loaded ${Math.min(tickers.length, markets.length)} ${this.orderlyExchangeName} markets`);

    this.store.update({
      tickers,
      loaded: { ...this.store.loaded, tickers: true },
    });

    this.publicWebsocket.connectAndSubscribe();

    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on ${this.orderlyExchangeName}`);

    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded ${this.orderlyExchangeName} orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        // after fetch account in case positionMode or leverage updated
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.store.update({ positions, loaded: { ...this.store.loaded, positions: true } });

        // positions will provide the following update:
        // free: free_collateral,
        // used: total_collateral_value - free_collateral,
        // total: total_collateral_value,
        const upnl = positions.length > 0 ? Math.round(sumBy(positions, 'unrealizedPnl') * 100) / 100 : 0;
        this.store.update({
          balance: {
            total: this.store.balance.total - upnl,
            used: this.store.balance.used - upnl,
            free: this.store.balance.free,
            upnl,
          },
          loaded: {
            ...this.store.loaded,
            balance: true,
          },
        });

        if (count % 4 === 0) {
          const orders = await this.fetchOrders();
          if (this.isDisposed) return;
          this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        }

        // if (count % 10 === 0) {
        //   const balance = await this.fetchBalance();
        //   if (this.isDisposed) return;

        //   // woo doesnt provides unrealized pnl in the account endpoint
        //   // we are computing this from the positions polling
        //   (balance as Writable<Balance>).upnl =
        //     positions.length > 0 ? Math.round(sumBy(positions, 'unrealizedPnl') * 100) / 100 : 0;

        //   // total balance from API already takes in account uPNL
        //   (balance as Writable<Balance>).total = balance.total - balance.upnl;

        //   this.store.update({
        //     balance: {
        //       total: this.store.balance.total,
        //       used: this.store.balance.used,
        //       free: this.store.balance.free,
        //       upnl: balance.upnl,
        //     },
        //     loaded: {
        //       ...this.store.loaded,
        //       balance: true,
        //     },
        //   });
        // }
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), this.getTickInterval());
      }
    }
  };

  fetchAndUpdateAccountLeverage = async () => {
    const {
      data: { data: accountData },
    } = await this.xhr.get<{ data: ACCOUNT_RESPONSE }>(ENDPOINTS.ACCOUNT);

    // store the account leverage
    // we will use this value to populate the positions leverage
    this.accountLeverage = accountData?.max_leverage || 20;

    // same with positionMode
    this._accountPositionMode = PositionMode.OneWay;
  };

  fetchAndUpdateMarginTypes = async () => {
    const response = await this.xhr.get<{ data: { rows?: Array<Record<string, any>> } }>(ENDPOINTS.MARGIN_MODES);
    const rows = response.data?.data?.rows;
    if (!Array.isArray(rows)) return;

    for (const row of rows) {
      if (typeof row?.symbol !== 'string') continue;
      const rawMarginType = v(row, 'default_margin_mode');
      if (rawMarginType !== 'CROSS' && rawMarginType !== 'ISOLATED') continue;
      this.marginTypeBySymbol.set(normalizeSymbol(row.symbol), this.mapMarginType(rawMarginType));
    }
  };

  fetchMarkets = async () => {
    // try {
    const {
      data: {
        data: { rows },
      },
    } = await this.xhr.get<{ data: { rows: Array<Record<string, any>> } }>(ENDPOINTS.MARKETS);

    const markets: Market[] = rows
      .filter((r) => r.symbol.startsWith('PERP_'))
      .map((r) => {
        const [, baseAsset, quoteAsset, ...broker] = r.symbol.split('_');
        const brokerSuffix = broker.length ? `_${broker.join('_')}` : '';

        const minLeverage = Number(v(r, 'min_leverage'));
        const baseInitialMarginRate = Number(v(r, 'base_imr'));
        const legacyMaxLeverage = Number(v(r, 'max_leverage'));
        const maxLeverage =
          Number.isFinite(baseInitialMarginRate) && baseInitialMarginRate > 0
            ? Math.round(1 / baseInitialMarginRate)
            : legacyMaxLeverage;
        const leverageLimits = {
          min: Number.isFinite(minLeverage) && minLeverage > 0 ? minLeverage : 1,
          max: Number.isFinite(maxLeverage) && maxLeverage > 0 ? maxLeverage : 100,
        };

        return {
          id: `${baseAsset}/${quoteAsset}:USDC${brokerSuffix}`,
          symbol: `${baseAsset}${quoteAsset}${brokerSuffix}`,
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
              ...leverageLimits,
            },
          },
          info: cloneDeep(r),
        };
      });

    return markets;
    // } catch (err: any) {
    //   this.emitAndLogError(err?.response?.data?.message || err?.message);
    //   return this.store.markets;
    // }
  };

  fetchTickers = async () => {
    try {
      const {
        data: {
          data: { rows },
        },
      } = await this.xhr.get<{ data: { rows: Array<Record<string, any>> } }>(ENDPOINTS.TICKERS);

      return rows.reduce((acc: Ticker[], row) => {
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
          info: row,
        };

        return [...acc, ticker];
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  fetchPositions = async () => {
    try {
      const {
        data: {
          data: { rows: positions, total_collateral_value, free_collateral },
        },
      } = await this.xhr.get<{
        data: {
          rows: Array<Record<string, any>>;
          margin_ratio: number;
          initial_margin_ratio: number;
          maintenance_margin_ratio: number;
          open_margin_ratio: number;
          current_margin_ratio_with_orders: number;
          initial_margin_ratio_with_orders: number;
          maintenance_margin_ratio_with_orders: number;
          total_collateral_value: number;
          free_collateral: number;
        };
      }>(ENDPOINTS.POSITIONS);

      const positionMode = await this.getPositionMode();
      const realPositions = positions.reduce<Position[]>((acc, p) => {
        if (!p.position_qty) return acc;
        const position = this.mapPosition(p, positionMode);
        return position ? [...acc, position] : acc;
      }, []);

      const fakePositions: Position[] = this.store.markets.reduce((acc: Position[], m) => {
        const marginType = this.getCurrentMarginType(m.symbol);
        const fakePositionsForSymbol = [
          {
            id: this.createOrderlyPositionId(m.symbol, PositionSide.Long, positionMode, marginType),
            symbol: m.symbol,
            side: PositionSide.Long,
            entryPrice: 0,
            notional: 0,
            leverage: this.accountLeverage,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            positionMode: positionMode,
            marginType,
          },
          {
            id: this.createOrderlyPositionId(m.symbol, PositionSide.Short, positionMode, marginType),
            symbol: m.symbol,
            side: PositionSide.Short,
            entryPrice: 0,
            notional: 0,
            leverage: this.accountLeverage,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            positionMode: positionMode,
            marginType,
          },
        ];

        acc.push(...fakePositionsForSymbol);
        return acc;
      }, []);

      this.store.update({
        balance: {
          ...this.store.balance,
          free: free_collateral,
          used: total_collateral_value - free_collateral,
          total: total_collateral_value,
        },
      });

      return uniqBy([...realPositions, ...fakePositions], 'id');
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.positions;
    }
  };

  private mapMarginType = (rawMarginType: unknown): MarginType => {
    return typeof rawMarginType === 'string' && rawMarginType.toUpperCase() === 'ISOLATED'
      ? MarginType.Isolated
      : MarginType.Cross;
  };

  private toOrderlyMarginMode = (marginType: MarginType): 'CROSS' | 'ISOLATED' => {
    return marginType === MarginType.Isolated ? 'ISOLATED' : 'CROSS';
  };

  private getCurrentMarginType = (symbol: string): MarginType => {
    const cached = this.marginTypeBySymbol.get(symbol);
    if (cached) return cached;

    const activeMarginTypes = uniqBy(
      this.store.positions.filter((position) => position.symbol === symbol && position.contracts > 0),
      'marginType',
    ).map((position) => position.marginType);
    return activeMarginTypes.length === 1 && activeMarginTypes[0] ? activeMarginTypes[0] : MarginType.Cross;
  };

  private createOrderlyPositionId = (
    symbol: string,
    side: PositionSide,
    positionMode: PositionMode,
    marginType: MarginType,
  ) => {
    const baseId = createPositionId(symbol, side, positionMode);
    return marginType === MarginType.Isolated ? `${baseId}:isolated` : baseId;
  };

  mapPosition = (p: Record<string, any>, positionMode: PositionMode = PositionMode.OneWay): Position | null => {
    const symbol = normalizeSymbol(p.symbol);
    const ticker = this.store.tickersBySymbol[symbol];
    if (!ticker) return null;

    const holdings = Number(v(p, 'position_qty'));
    if (!Number.isFinite(holdings)) return null;

    const entryPrice = Number(v(p, 'average_open_price')) || 0;
    const contracts = Math.abs(holdings);
    const side = holdings >= 0 ? PositionSide.Long : PositionSide.Short;
    const markPrice = Number(v(p, 'mark_price')) || ticker.mark;
    const notional = contracts * markPrice;

    const priceDiff = Math.max(entryPrice, markPrice) - Math.min(entryPrice, markPrice);
    const isLoss =
      (side === PositionSide.Long && markPrice < entryPrice) || (side === PositionSide.Short && markPrice > entryPrice);
    const absUPNL = priceDiff * contracts;
    const upnl = contracts > 0 ? Math.round((isLoss ? -absUPNL : absUPNL) * 100) / 100 : 0;

    const marginType = this.mapMarginType(v(p, 'margin_mode'));
    const rawLeverage = Number(v(p, 'leverage'));
    const imr = Number(v(p, 'imr'));
    const leverage =
      Number.isFinite(rawLeverage) && rawLeverage > 0
        ? rawLeverage
        : Number.isFinite(imr) && imr > 0
          ? 1 / imr
          : this.accountLeverage;
    const rawMargin = Number(v(p, 'margin'));

    return {
      id: this.createOrderlyPositionId(symbol, side, positionMode, marginType),
      symbol,
      side,
      entryPrice,
      notional,
      leverage,
      unrealizedPnl: upnl,
      contracts,
      liquidationPrice: Number(v(p, 'est_liq_price')) || 0,
      marginSize: Number.isFinite(rawMargin) && rawMargin > 0 ? rawMargin : leverage > 0 ? notional / leverage : 0,
      positionMode,
      marginType,
      info: cloneDeep(p),
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };
  };

  fetchOrders = async () => {
    const limitOrders = await this.fetchLimitOrders();
    const algoOrders = await this.fetchAlgoOrders();
    return [...limitOrders, ...algoOrders];
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const {
      data: {
        data: { rows },
      },
    } = await this.xhr.get<{ data: { rows: Array<Record<string, any>> } }>(ENDPOINTS.KLINE, {
      params: {
        symbol: reverseSymbol(opts.symbol),
        type: opts.interval,
        limit: Math.min(opts.limit || 500, 1000),
      },
    });

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
    if (this.isAlgoOrder(order.type)) {
      return this.updateAlgoOrder({ order, update });
    }

    const own = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);
    const fail = (reason: string): never => {
      const err = new Error(`Orderly native amend ${reason}`);
      this.emitAndLogError(err);
      throw err;
    };

    const updateKeys = Object.keys(update);
    if (updateKeys.length !== 1 || (updateKeys[0] !== 'price' && updateKeys[0] !== 'amount')) {
      fail('requires exactly one price or quantity change');
    }

    const current =
      this.store.orders.find((candidate) => candidate.id === order.id && candidate.symbol === order.symbol) ??
      fail('requires an exactly reconciled store order');

    const stableFields: ReadonlyArray<
      keyof Pick<
        Order,
        'status' | 'symbol' | 'type' | 'side' | 'price' | 'amount' | 'filled' | 'remaining' | 'reduceOnly'
      >
    > = ['status', 'symbol', 'type', 'side', 'price', 'amount', 'filled', 'remaining', 'reduceOnly'];
    if (stableFields.some((field) => !Object.is(current[field], order[field]))) {
      fail('requires the submitted order to match the reconciled store row');
    }

    if (
      current.status !== OrderStatus.Open ||
      current.type !== OrderType.Limit ||
      current.parentId != null ||
      current.close === true ||
      current.takeProfit != null ||
      current.stopLoss != null ||
      (current.timeInForce != null && current.timeInForce !== OrderTimeInForce.GoodTillCancel)
    ) {
      fail('supports only ordinary open GTC LIMIT orders');
    }

    const market = this.store.marketsBySymbol[current.symbol];
    if (!market) fail('requires reconciled market precision');
    if (
      !Number.isFinite(market.precision.price) ||
      market.precision.price <= 0 ||
      !Number.isFinite(market.precision.amount) ||
      market.precision.amount <= 0
    ) {
      fail('requires valid market precision');
    }

    const raw = current.info;
    if (raw == null || typeof raw !== 'object' || Array.isArray(raw)) {
      fail('requires a complete REST order row');
    }

    const requiredRestFields = [
      'order_id',
      'symbol',
      'type',
      'side',
      'status',
      'price',
      'quantity',
      'total_executed_quantity',
      'visible_quantity',
      'reduce_only',
      'margin_mode',
    ];
    if (requiredRestFields.some((field) => !own(raw, field))) {
      fail('requires explicit REST identity, fill, visibility, reduce-only, and margin-mode provenance');
    }

    const rawOrderId = raw.order_id;
    const canonicalRawOrderId =
      typeof rawOrderId === 'number'
        ? Number.isSafeInteger(rawOrderId) && rawOrderId > 0
          ? String(rawOrderId)
          : null
        : typeof rawOrderId === 'string' && /^[1-9]\d*$/.test(rawOrderId)
          ? rawOrderId
          : null;

    const expectedSide = current.side === OrderSide.Buy ? 'BUY' : current.side === OrderSide.Sell ? 'SELL' : null;
    const stableOrderId = canonicalRawOrderId ?? fail('requires exact immutable order identity parity');
    if (
      stableOrderId !== current.id ||
      current.id !== order.id ||
      raw.symbol !== reverseSymbol(current.symbol) ||
      raw.type !== 'LIMIT' ||
      raw.side !== expectedSide
    ) {
      fail('requires exact immutable order identity parity');
    }

    if (raw.status !== 'NEW') fail('requires an unfilled pending REST order');
    if (raw.reduce_only !== current.reduceOnly || typeof raw.reduce_only !== 'boolean') {
      fail('requires explicit reduce-only parity');
    }
    if (raw.margin_mode !== 'CROSS' && raw.margin_mode !== 'ISOLATED') {
      fail('requires proven CROSS or ISOLATED margin provenance');
    }

    const executionFields = [
      raw.total_executed_quantity,
      ...(own(raw, 'executed_quantity') ? [raw.executed_quantity] : []),
      ...(own(raw, 'executed') ? [raw.executed] : []),
    ];
    if (
      executionFields.some((value) => typeof value !== 'number' || !Object.is(value, 0)) ||
      !Object.is(current.filled, 0) ||
      current.remaining !== current.amount
    ) {
      fail('requires exact zero-fill state');
    }

    if (
      typeof raw.price !== 'number' ||
      !Number.isFinite(raw.price) ||
      raw.price <= 0 ||
      typeof raw.quantity !== 'number' ||
      !Number.isFinite(raw.quantity) ||
      raw.quantity <= 0 ||
      typeof raw.visible_quantity !== 'number' ||
      !Number.isFinite(raw.visible_quantity) ||
      raw.visible_quantity !== raw.quantity ||
      raw.price !== current.price ||
      raw.quantity !== current.amount ||
      adjust(raw.price, market.precision.price) !== raw.price ||
      adjust(raw.quantity, market.precision.amount) !== raw.quantity
    ) {
      fail('requires exact ordinary LIMIT price, quantity, visibility, and precision parity');
    }

    if (
      (own(raw, 'time_in_force') && raw.time_in_force !== 'GTC') ||
      ['algo_order_id', 'root_algo_order_id', 'trigger_price', 'algo_type'].some(
        (field) => own(raw, field) && raw[field] != null,
      )
    ) {
      fail('cannot preserve an unsupported time-in-force or trigger shape');
    }

    const change = updateKeys[0] as 'price' | 'amount';
    const requested = change === 'price' ? (update as { price: number }).price : (update as { amount: number }).amount;
    const step = change === 'price' ? market.precision.price : market.precision.amount;
    if (typeof requested !== 'number' || !Number.isFinite(requested) || requested <= 0) {
      fail('requires a finite positive amended value');
    }
    const amended = adjust(requested, step);
    if (amended <= 0 || amended !== requested) {
      fail('requires an amended value aligned exactly to market precision');
    }
    if (change === 'amount' && (amended < market.limits.amount.min || amended > market.limits.amount.max)) {
      fail('quantity is outside the reconciled market limits');
    }
    if (amended === current[change]) fail('requires a real change after market-precision validation');

    const payload: EDIT_ORDER_REQUEST = {
      order_id: stableOrderId,
      symbol: raw.symbol,
      order_type: raw.type,
      side: raw.side,
      ...(change === 'price' ? { order_price: amended } : { order_quantity: amended }),
    };

    try {
      const { data } = await this.xhr.put<EDIT_ORDER_RESPONSE>(ENDPOINTS.UPDATE_ORDER, payload);
      if (data?.success !== true || data.data?.status !== 'EDIT_SENT') {
        throw new Error(data?.message || `Orderly native amend returned ${data?.data?.status || 'no status'}`);
      }
      return [current.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const payload: Record<string, any> = {};

    payload.order_id = +(order.parentId || 0);

    if ('price' in update) {
      payload.child_orders = [{ order_id: order.id, trigger_price: +update.price }];
    }

    try {
      await this.xhr.put(`${ENDPOINTS.ALGO_ORDER}`, payload);
      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchLimitOrders = async () => {
    try {
      const {
        data: { data },
      } = await this.xhr.get<{ data: { rows: Array<Record<string, any>> } }>(ENDPOINTS.ORDERS, {
        params: { status: 'INCOMPLETE' },
      });

      return data.rows.reduce<Order[]>((acc, o) => {
        const order = this.mapLimitOrder(o);
        return order !== null ? [...acc, order] : acc;
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  mapLimitOrder = (o: Record<string, any>) => {
    const symbol = normalizeSymbol(o.symbol);
    const market = this.store.marketsBySymbol[symbol];
    if (!market) return null;
    const quantity = o.quantity || o.order_quantity;
    const executed = v(o, 'totalExecutedQuantity') ?? v(o, 'executed') ?? 0;

    const order: Order = {
      id: `${v(o, 'order_id') || v(o, 'orderId')}`,
      status: OrderStatus.Open,
      symbol,
      type: ORDER_TYPE[o.type || o.order_type],
      side: ORDER_SIDE[o.side],
      price: o.price || o.order_price,
      amount: quantity,
      reduceOnly: o.reduce_only ?? o.reduceOnly,
      filled: executed,
      remaining: subtract(quantity, executed),
      lastUpdate: virtualClock.getCurrentTimeMs(),
      info: o,
    };

    return order;
  };

  fetchAlgoOrders = async () => {
    try {
      const {
        data: {
          data: { rows },
        },
      } = await this.xhr.get<{
        data: { rows: Array<Record<string, any>> };
      }>(ENDPOINTS.ALGO_ORDERS, { params: { status: 'INCOMPLETE' } });

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

    const arr = o.childOrders || o.child_orders ? o.childOrders || o.child_orders : [o];
    return arr
      .filter(
        (co: Record<string, any>) =>
          v(co, 'trigger_price') ||
          v(co, 'triggerPrice') ||
          v(co, 'algo_type') === 'TRAILING_STOP' ||
          v(co, 'algoType') === 'TRAILING_STOP',
      )
      .map((co: Record<string, any>) => {
        const filled = v(co, 'total_executed_quantity') || v(co, 'totalExecutedQuantity');
        const price =
          v(co, 'trigger_price') || v(co, 'triggerPrice') || subtract(v(co, 'extreme_price'), v(co, 'callbackValue'));

        return {
          id: `${v(co, 'algo_order_id')}`,
          parentId: `${v(o, 'root_algo_order_id') || v(co, 'root_algo_order_id') || v(o, 'rootAlgoOrderId') || v(co, 'rootAlgoOrderId')}`,
          status: OrderStatus.Open,
          symbol,
          type: ORDER_TYPE[v(co, 'algo_type') || v(co, 'algoType')],
          side: ORDER_SIDE[co.side],
          price,
          amount: co.quantity,
          reduceOnly: v(co, 'reduce_only') || v(co, 'reduceOnly'),
          filled,
          remaining: subtract(co.quantity, filled),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          info: {
            ...cloneDeep(co),
            margin_mode: v(co, 'margin_mode') ?? v(o, 'margin_mode'),
          },
        };
      });
  };

  cancelOrders = async (orders: Order[]) => {
    await forEachSeries(orders, async (order) => {
      const isAlgo = this.isAlgoOrder(order.type);

      if (isAlgo) {
        await this.cancelAlgoOrder(order);
      } else {
        await this.xhr.delete(
          `${ENDPOINTS.CANCEL_ORDER}?order_id=${parseInt(order.id, 10)}&symbol=${reverseSymbol(order.symbol)}`,
          { data: {} },
          //   , {
          //   data: omitUndefined({
          //     order_id: parseInt(order.id, 10),
          //     symbol: reverseSymbol(order.symbol),
          //   }),
          // }
        );
      }
    });
  };

  cancelAlgoOrder = async (order: Order) => {
    try {
      const sibling = this.store.orders.find((o) => o.parentId === order.parentId && o.id !== order.id);

      const id = order.parentId || order.id;
      await this.xhr.delete(`${ENDPOINTS.ALGO_ORDER}?order_id=${id}&symbol=${reverseSymbol(order.symbol)}`, {
        data: {},
      });

      if (sibling) {
        const priceKey = sibling.type === OrderType.StopLoss ? 'stopLoss' : 'takeProfit';
        const siblingMarginType = this.mapMarginType(v(sibling.info, 'margin_mode'));

        await this.placePositionalAlgoOrder({
          symbol: sibling.symbol,
          side: sibling.side,
          [priceKey]: sibling.price,
          marginMode: this.toOrderlyMarginMode(siblingMarginType),
        });
      }
    } catch (err: any) {
      if (err?.response?.data?.message?.includes('already canceled')) {
        this.store.updateOrderStatus({ id: order.id }, OrderStatus.Canceled);
      } else {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
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
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
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
    const type = inverseObj(ORDER_TYPE)[opts.type];
    const marginType = this.resolveMarginTypeForOrder(opts);
    const marginMode = this.toOrderlyMarginMode(marginType);

    // Determine order_tag based on exchange type
    let orderTag = 'TEALSTREET';
    if (this.orderlyExchangeName === 'SALSADEX') {
      orderTag = 'TEALSALSA';
    } else if (this.orderlyExchangeName === 'KODIAKFI') {
      orderTag = 'TEALKODIAK';
    } else if (this.orderlyExchangeName === 'MODE') {
      orderTag = 'TEALMODE';
    } else if (this.orderlyExchangeName === 'WHAT') {
      orderTag = 'TEALWHAT';
    }

    const req = omitUndefined({
      symbol: reverseSymbol(opts.symbol),
      order_type: type,
      order_price: opts.type === OrderType.Limit ? price : undefined,
      reduce_only: opts.reduceOnly,
      side: inverseObj(ORDER_SIDE)[opts.side],
      position_side: await this.getPositionSideForOrder(opts),
      margin_mode: marginMode,
      order_tag: orderTag,
      client_order_id: callerClientOrderId,
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);
    const payloads = times(lots, () => {
      return { ...req, order_quantity: lotSize };
    });

    if (rest) payloads.push({ ...req, order_quantity: rest });
    this.assertSingleNativeClientOrderId(callerClientOrderId, payloads.length);

    try {
      const orderIds = await mapSeries(payloads, async (payload) => {
        const {
          data: { data },
        } = await this.xhr.post<PLACE_ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDER, payload);

        return String(data.order_id);
      });

      if (opts.stopLoss || opts.takeProfit) {
        const algoIds = await this.placePositionalAlgoOrder({
          ...opts,
          side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          marginMode,
        });
        orderIds.push(...algoIds);
      }

      return orderIds;
    } catch (err: any) {
      const message = err?.response?.data?.message || err?.message;
      this.emitAndLogError(err, message, getOrderlyOrderErrorLogOptions(message));
      throw err;
    }
  };

  placeAlgoOrder = async (opts: PlaceOrderOpts) => {
    if (opts.type === OrderType.TrailingStopLoss) {
      const error = new Error('Orderly trailing-stop placement is not supported');
      this.emitAndLogError(error);
      throw error;
    }

    if (!opts.price) {
      this.emitAndLogError('Price is required for algo orders');
      return [];
    }

    const marginMode = this.toOrderlyMarginMode(this.resolveMarginTypeForOrder({ ...opts, reduceOnly: true }));

    const sibling = this.store.orders.find(
      (o) =>
        o.symbol === opts.symbol &&
        (o.type === OrderType.StopLoss || o.type === OrderType.TakeProfit) &&
        o.status === OrderStatus.Open &&
        this.toOrderlyMarginMode(this.mapMarginType(v(o.info, 'margin_mode'))) === marginMode,
    );

    let stopLoss = opts.type === OrderType.StopLoss ? opts.price : undefined;
    let takeProfit = opts.type === OrderType.TakeProfit ? opts.price : undefined;

    // TODO: race condition with bulk cancel stops where we recreate sibling stop after cancel all still
    if (sibling) {
      if (sibling.type === OrderType.StopLoss) stopLoss = sibling.price;
      if (sibling.type === OrderType.TakeProfit) takeProfit = sibling.price;
      await this.cancelAlgoOrder(sibling);
    }

    const params: Pick<PlaceOrderOpts, 'symbol' | 'side' | 'stopLoss' | 'takeProfit'> = {
      symbol: opts.symbol,
      side: opts.side,
      ...(typeof stopLoss === 'number' ? { stopLoss } : {}),
      ...(typeof takeProfit === 'number' ? { takeProfit } : {}),
    };

    return await this.placePositionalAlgoOrder({
      ...params,
      marginMode,
    });
  };

  private resolveMarginTypeForOrder = (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'symbol'>): MarginType => {
    const cached = this.marginTypeBySymbol.get(opts.symbol);
    if (!opts.reduceOnly) return cached ?? this.getCurrentMarginType(opts.symbol);

    const closingSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const candidates = this.store.positions.filter(
      (position) => position.symbol === opts.symbol && position.side === closingSide && position.contracts > 0,
    );
    if (cached) {
      const selected = candidates.filter((position) => position.marginType === cached);
      if (selected.length === 1) return cached;
    }
    if (candidates.length === 1) return candidates[0].marginType ?? MarginType.Cross;
    if (candidates.length > 1) {
      throw new Error(
        `Orderly reduce-only order for ${opts.symbol} is ambiguous across Cross and Isolated positions; select the intended margin mode first`,
      );
    }
    return cached ?? this.getCurrentMarginType(opts.symbol);
  };

  placePositionalAlgoOrder = async (
    opts: Pick<PlaceOrderOpts, 'side' | 'stopLoss' | 'symbol' | 'takeProfit'> & {
      marginMode?: 'CROSS' | 'ISOLATED';
    },
  ) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }
    const pPrice = market.precision.price;
    const otherProps = omitUndefined({
      reduce_only: true,
      // not supported due to one way
      // positionSide: await this.getPositionSideForOrder({
      //   ...opts,
      //   reduceOnly: true,
      // }),
    });

    const req: Record<string, any> = {
      ...otherProps,
      symbol: reverseSymbol(opts.symbol),
      algo_type: 'POSITIONAL_TP_SL',
      margin_mode: opts.marginMode ?? this.toOrderlyMarginMode(this.getCurrentMarginType(opts.symbol)),
      child_orders: [],
    };

    if (typeof opts.stopLoss === 'number') {
      req.child_orders.push({
        ...otherProps,
        algo_type: 'STOP_LOSS',
        type: 'CLOSE_POSITION',
        side: opts.side === OrderSide.Sell ? 'SELL' : 'BUY',
        trigger_price: `${adjust(opts.stopLoss, pPrice)}`,
      });
    }

    if (typeof opts.takeProfit === 'number') {
      req.child_orders.push({
        ...otherProps,
        algo_type: 'TAKE_PROFIT',
        type: 'CLOSE_POSITION',
        side: opts.side === OrderSide.Sell ? 'SELL' : 'BUY',
        trigger_price: `${adjust(opts.takeProfit, pPrice)}`,
      });
    }

    try {
      const {
        data: { data },
      } = await this.xhr.post<{
        data: { rows: Array<Record<string, any>> };
      }>(ENDPOINTS.ALGO_ORDER, req);

      return data.rows.map((r) => String(r.orderId));
    } catch (err: any) {
      const message = err?.response?.data?.message || err?.message;
      this.emitAndLogError(message, undefined, getOrderlyOrderErrorLogOptions(message));
      return [];
    }
  };

  cancelAllOrders = async () => {
    const cancellations = [
      () => this.xhr.delete(ENDPOINTS.CANCEL_ORDERS),
      () => this.xhr.delete(ENDPOINTS.CANCEL_ALGO_ORDERS),
    ];

    for (const cancel of cancellations) {
      try {
        await cancel();
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
      }
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    const orderlySymbol = reverseSymbol(symbol);
    const cancellations = [
      () => this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, { params: { symbol: orderlySymbol } }),
      () => this.xhr.delete(ENDPOINTS.CANCEL_ALGO_ORDERS, { params: { symbol: orderlySymbol } }),
    ];

    for (const cancel of cancellations) {
      try {
        await cancel();
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
      }
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!Number.isFinite(inputLeverage)) throw new Error('Orderly leverage must be a finite number');

    const marginType = this.getCurrentMarginType(symbol);
    const clamped = Math.min(market.limits.leverage.max, Math.max(market.limits.leverage.min, inputLeverage));
    const leverage = Math.round(clamped);
    const payload = {
      leverage,
      symbol: reverseSymbol(symbol),
      margin_mode: this.toOrderlyMarginMode(marginType),
    };

    try {
      const { data } = await this.xhr.post(ENDPOINTS.LEVERAGE, payload);
      if (data?.success !== true) throw new Error(data?.message || 'Orderly leverage update failed');
      if (marginType === MarginType.Cross) this.accountLeverage = leverage;
      this.store.updatePositions(
        this.store.positions
          .filter((position) => position.symbol === symbol && position.marginType === marginType)
          .map((position) => [position, { leverage }] as [Position, Partial<Position>]),
      );
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  setAllLeverage = async (inputLeverage: number) => {
    if (!Number.isFinite(inputLeverage)) throw new Error('Orderly leverage must be a finite number');
    const clamped = Math.min(100, Math.max(1, inputLeverage));
    const leverage = Math.round(clamped);

    if (this.accountLeverage !== leverage && !this.isDisposed) {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.LEVERAGE, { leverage, margin_mode: 'CROSS' });
        if (data?.success !== true) throw new Error(data?.message || 'Orderly leverage update failed');
        this.accountLeverage = leverage;
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
      }
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) throw new Error('Orderly margin mode update requires a symbol');
    if (!this.store.marketsBySymbol[symbol]) throw new Error(`Market ${symbol} not found`);

    const marginType = isolated ? MarginType.Isolated : MarginType.Cross;
    try {
      const { data } = await this.xhr.post(ENDPOINTS.MARGIN_MODE, {
        symbol: reverseSymbol(symbol),
        default_margin_mode: this.toOrderlyMarginMode(marginType),
      });
      if (data?.success !== true) throw new Error(data?.message || 'Orderly margin mode update failed');
      this.marginTypeBySymbol.set(symbol, marginType);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  private isAlgoOrder = (orderType: OrderType) => {
    return (
      orderType === OrderType.StopLoss || orderType === OrderType.TakeProfit || orderType === OrderType.TrailingStopLoss
    );
  };
}
