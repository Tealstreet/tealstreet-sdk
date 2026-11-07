import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import BigNumber from 'bignumber.js';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import { forEachSeries } from 'p-iteration';

import {
  Balance,
  Candle,
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
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  RemoteSignerRequest,
  RemoteSignerResponse,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { loop } from '../../../utils/loop';
import { adjust, multiply, round } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './coinbase.api';
import { mapCoinbaseRestOrder } from './coinbase.orders';
import {
  BALANCE_RESPONSE,
  BALANCE_RESPONSE_CFM,
  CANDLES_RESPONSE,
  EDIT_ORDER_REQUEST,
  EDIT_ORDER_RESPONSE,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTX_INSTRUMENTS_RESPONSE,
  LIST_ORDERS_RESPONSE,
  LIST_PORTFOLIOS_RESPONSE,
  LIST_POSITIONS_CFM_RESPONSE,
  LIST_POSITIONS_INTX_RESPONSE,
  MARKET_RESPONSE,
  PLACE_ORDER_REQUEST,
  PLACE_ORDER_RESPONSE,
  PORTFOLIO_BREAKDOWN_RESPONSE,
} from './coinbase.types';
import {
  normalizeCfmContractSize,
  normalizeCoinbaseUsdEquivalentProductId,
  resolveCoinbaseUsdEquivalentMarket,
  sign,
} from './coinbase.utils';
import { CoinbasePrivateWebsocket } from './coinbase.ws-private';
import { CoinbasePublicWebsocket } from './coinbase.ws-public';

const COINBASE_UNSIGNED_DECIMAL = /^(?:0|[1-9]\d*)(?:\.\d+)?$/;
const COINBASE_ZERO_DECIMAL = /^0(?:\.0+)?$/;
const COINBASE_OPEN_ORDER_STATUSES = ['PENDING', 'OPEN', 'QUEUED', 'CANCEL_QUEUED', 'EDIT_QUEUED'];
const COINBASE_ORDER_PAGE_LIMIT = 100;
const COINBASE_ORDER_POLL_INTERVAL_MS = 10_000;
const COINBASE_ORDER_RATE_LIMIT_BACKOFF_MS = 60_000;
const COINBASE_ORDER_WRITE_RECONCILE_DELAY_MS = 2_000;
const COINBASE_PRIVATE_USER_STREAM_WRITE_GRACE_MS = 8_000;
const COINBASE_PRIVATE_USER_STREAM_DEGRADED_MESSAGE =
  'Coinbase private WS is not receiving live order updates. Orders will reconcile through REST; if your Coinbase key restricts IPs, whitelist both Tealstreet proxy IPs and your current browser IP.';
const COINBASE_PRIVATE_USER_STREAM_AUTH_DEGRADED_MESSAGE =
  'Coinbase private WS authentication failed. REST trading can continue, but live order updates require your current browser IP to be whitelisted or a Coinbase WS proxy route.';
// REST replication for a freshly placed order can lag WS by a few hundred ms.
// Preserve open WS-only orders whose lastUpdate is within this window so a
// snapshot poll doesn't briefly evict them from the store on a race.
const COINBASE_ORDER_REST_LAG_TOLERANCE_MS = 5_000;

const isCoinbaseUnsignedDecimal = (value: unknown): value is string =>
  typeof value === 'string' && COINBASE_UNSIGNED_DECIMAL.test(value);

const isCoinbaseZeroDecimal = (value: unknown): value is string =>
  typeof value === 'string' && COINBASE_ZERO_DECIMAL.test(value);

export class CoinbaseExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Coinbase', /^\S+$/, {
    mint: () => 'tealstreet-' + uuid(),
  });

  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: CoinbasePublicWebsocket;
  privateWebsocket: CoinbasePrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  selectedPortfolio: LIST_PORTFOLIOS_RESPONSE['portfolios'][0] | null = null;
  private nextOrdersPollAt = 0;
  private orderWriteReconcileTimeout: NodeJS.Timeout | undefined;
  private privateUserStreamWriteCheckTimeout: NodeJS.Timeout | undefined;
  private lastPrivateUserStreamMessageAt = 0;
  private hasWarnedPrivateUserStreamDegraded = false;

  supportSpot = true;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new CoinbasePublicWebsocket(this);
    this.privateWebsocket = new CoinbasePrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    if (this.orderWriteReconcileTimeout) {
      clearTimeout(this.orderWriteReconcileTimeout);
      this.orderWriteReconcileTimeout = undefined;
    }
    if (this.privateUserStreamWriteCheckTimeout) {
      clearTimeout(this.privateUserStreamWriteCheckTimeout);
      this.privateUserStreamWriteCheckTimeout = undefined;
    }
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  signWsAuth = async (request: RemoteSignerRequest): Promise<RemoteSignerResponse> => {
    if (request.wsAuth?.outputFormat !== 'coinbase-jwt') {
      return { id: request.id, error: `Unsupported Coinbase WS auth outputFormat: ${request.wsAuth?.outputFormat}` };
    }
    if (!this.options.key || !this.options.secret) {
      return { id: request.id, error: 'No Coinbase key or secret available for WS auth signing' };
    }

    try {
      return { id: request.id, wsAuthSignature: await sign(this.options.key, this.options.secret) };
    } catch (err: any) {
      return { id: request.id, error: err?.message || 'Coinbase WS auth signing failed' };
    }
  };

  getAccount = async () => {
    if (!this.selectedPortfolio) {
      this.selectedPortfolio = await this.findSelectedPortfolio();
    }

    return { userId: this.selectedPortfolio.uuid };
  };

  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.MARKETS);
      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    // need this first to know what types of data to fetch (skip in public-only mode)
    if (!this.isPublicDataOnly) {
      this.selectedPortfolio = await this.findSelectedPortfolio();
    }

    if (!useExternalPublicData) {
      // load initial market data
      // then we can poll for live data
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      await this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      this.publicWebsocket.connectAndSubscribe();
    }

    if (!this.isPublicDataOnly) {
      this._startPrivate();
    } else {
      await this._startPublicDataOnly();
    }
  };

  _startPrivate = async () => {
    this.privateWebsocket.connectAndSubscribe();

    // // fetch current position mode (Hedge/One-way)
    // this.store.setSetting('isHedged', await this.fetchPositionMode());
    //
    // start ticking live data
    // balance, tickers, positions
    if (!this.selectedPortfolio) {
      this.selectedPortfolio = await this.findSelectedPortfolio();
    }
    if (this.isDisposed) return;

    await this.tick();
    if (this.isDisposed) return;

    //
    // this.log(`Ready to trade on coinbase`);

    //
    // // fetch unfilled orders
    // const orders = await this.fetchOrders();
    // if (this.isDisposed) return;
    //
    // this.log(`Loaded Binance orders`);
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

    this.log(`Ready to watch public data on Coinbase`);

    // Fetch tickers from public INTX endpoint
    const tickers = this.replayPublicSpot
      ? this.formatSpotCfmTicker(await this.fetchReplayPublicSpotProducts())
      : await this.fetchTickersIntx();
    this.store.update({
      tickers,
      orders: [],
      balance: {
        used: 0,
        free: 0,
        total: 0,
        upnl: 0,
      },
      positions: [],
      loaded: { ...this.store.loaded, tickers: true, orders: true, balance: true, positions: true },
    });
    if (this.replayPublicSpot) loop(() => this.tickReplayPublicSpot(), this.getTickInterval());
  };

  tick = async () => {
    if (!this.isDisposed) {
      try {
        const tickers = await this.fetchTickers();
        if (this.isDisposed) return;
        this.store.update({
          tickers,
          loaded: {
            ...this.store.loaded,
            tickers: true,
          },
        });

        // iterate over tickers and update OB and kline callbacks
        const messageHandlers = this.publicWebsocket.messageHandlers;
        tickers.forEach((ticker) => {
          if (messageHandlers[`poll_book.${ticker.symbol}`]) {
            messageHandlers[`poll_book.${ticker.symbol}`].forEach((cb) => cb(ticker));
          }
          if (messageHandlers[`poll_kline.${ticker.symbol}`]) {
            messageHandlers[`poll_kline.${ticker.symbol}`].forEach((cb) => cb(ticker));
          }
        });

        const balance = await this.fetchBalance();
        if (this.isDisposed) return;
        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });

        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.store.update({
          positions,
          loaded: { ...this.store.loaded, positions: true },
        });

        if (this.shouldPollOrders()) {
          try {
            const orders = await this.fetchOpenOrdersSnapshot();
            if (this.isDisposed) return;
            this.store.update({
              orders: this.mergeRestOrdersWithFreshWsState(orders),
              loaded: { ...this.store.loaded, orders: true },
            });
          } catch (err: any) {
            this.emitOrdersFetchError(err);
          }
        }
      } catch (err: any) {
        this.emitAndLogError(err, err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };

  private shouldPollOrders = () => {
    if (this.store.loaded.orders) {
      return false;
    }

    const now = Date.now();
    if (now >= this.nextOrdersPollAt) {
      this.nextOrdersPollAt = now + COINBASE_ORDER_POLL_INTERVAL_MS;
      return true;
    }
    return false;
  };

  markPrivateUserStreamMessageReceived = () => {
    this.lastPrivateUserStreamMessageAt = Date.now();
    this.hasWarnedPrivateUserStreamDegraded = false;
    this.store.updateSocketHealth({ privateDegradedReason: undefined });
  };

  markPrivateUserStreamDegraded = (reason = COINBASE_PRIVATE_USER_STREAM_DEGRADED_MESSAGE, forceToast = false) => {
    this.store.updateSocketHealth({ privateDegradedReason: reason });
    this.log(reason, LogSeverity.Warning);
    if (forceToast && !this.hasWarnedPrivateUserStreamDegraded) {
      this.hasWarnedPrivateUserStreamDegraded = true;
      this.emitter.emit('warning', reason, { forceToast: true });
    }
  };

  markPrivateUserStreamAuthFailed = (message?: string) => {
    const detail = message ? ` Coinbase message: ${message}` : '';
    this.markPrivateUserStreamDegraded(`${COINBASE_PRIVATE_USER_STREAM_AUTH_DEGRADED_MESSAGE}${detail}`);
  };

  private schedulePrivateUserStreamWriteCheck = () => {
    const lastUserFrameAtSchedule = this.lastPrivateUserStreamMessageAt;
    if (this.privateUserStreamWriteCheckTimeout) {
      clearTimeout(this.privateUserStreamWriteCheckTimeout);
    }
    this.privateUserStreamWriteCheckTimeout = setTimeout(() => {
      this.privateUserStreamWriteCheckTimeout = undefined;
      if (this.isDisposed || this.lastPrivateUserStreamMessageAt > lastUserFrameAtSchedule) {
        return;
      }

      this.markPrivateUserStreamDegraded(COINBASE_PRIVATE_USER_STREAM_DEGRADED_MESSAGE, true);
    }, COINBASE_PRIVATE_USER_STREAM_WRITE_GRACE_MS);
  };

  private reconcileOpenOrdersFromRest = async (reason: string) => {
    try {
      const orders = await this.fetchOpenOrdersSnapshot();
      if (this.isDisposed) return;
      this.store.update({
        orders: this.mergeRestOrdersWithFreshWsState(orders),
        loaded: { ...this.store.loaded, orders: true },
      });
    } catch (err: any) {
      this.log(`Coinbase ${reason} REST order reconciliation failed`, LogSeverity.Warning);
      this.emitOrdersFetchError(err);
    }
  };

  private scheduleOpenOrdersRestReconciliation = (reason: string) => {
    if (this.isPublicDataOnly) return;
    if (this.orderWriteReconcileTimeout) {
      clearTimeout(this.orderWriteReconcileTimeout);
    }
    this.orderWriteReconcileTimeout = setTimeout(() => {
      this.orderWriteReconcileTimeout = undefined;
      void this.reconcileOpenOrdersFromRest(reason);
    }, COINBASE_ORDER_WRITE_RECONCILE_DELAY_MS);
  };

  /**
   * Merge a REST orders snapshot with any WS-only in-flight orders currently in
   * the store. Freshly placed orders often reach us over WS before Coinbase's
   * REST listing catches up; a wholesale replace briefly evicts them until the
   * next WS event or poll. We preserve store orders that (a) are missing from
   * the REST snapshot, (b) are still Open, and (c) were last updated within
   * `COINBASE_ORDER_REST_LAG_TOLERANCE_MS` — anything older than that we treat
   * as authoritatively gone from REST's point of view.
   */
  private mergeRestOrdersWithFreshWsState = (restOrders: Order[]): Order[] => {
    const restIds = new Set(restOrders.map((order) => order.id));
    const now = Date.now();
    const preservedInFlight = this.store.orders.filter(
      (existing) =>
        !restIds.has(existing.id) &&
        existing.status === OrderStatus.Open &&
        now - existing.lastUpdate < COINBASE_ORDER_REST_LAG_TOLERANCE_MS,
    );
    if (preservedInFlight.length === 0) return restOrders;
    return [...restOrders, ...preservedInFlight];
  };

  private buildOptimisticOrderFromCreate = (
    payload: PLACE_ORDER_REQUEST,
    orderId: string,
    response: PLACE_ORDER_RESPONSE,
  ): Order | undefined => {
    if (!payload.product_id || !payload.order_configuration) return undefined;
    const market = this.resolveMarketByCoinbaseProductId(payload.product_id);
    if (!market || market.isSpot !== true || market.isExpiring === true || market.contractSize !== 1) return undefined;

    const config = response.order_configuration ?? payload.order_configuration;
    const limit = config.limit_limit_gtc || config.limit_limit_gtd || config.sor_limit_ioc;
    if (!limit) return undefined;

    const amount = Number(limit.base_size);
    const price = Number(limit.limit_price);
    if (!Number.isFinite(amount) || amount <= 0 || !Number.isFinite(price) || price <= 0) return undefined;

    return {
      id: orderId,
      parentId: payload.client_order_id,
      status: OrderStatus.Open,
      symbol: market.symbol,
      type: OrderType.Limit,
      side: payload.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
      price,
      amount,
      filled: 0,
      remaining: amount,
      extra: {
        remainingNative: amount,
      },
      reduceOnly: false,
      info: { ...response, optimisticFromCreateOrder: true },
      lastUpdate: Date.now(),
    };
  };

  private emitOrdersFetchError = (err: any) => {
    if (err?.response?.status === 429) {
      this.nextOrdersPollAt = Date.now() + COINBASE_ORDER_RATE_LIMIT_BACKOFF_MS;
      this.log('Coinbase open-order REST fallback rate limited (429); waiting before retrying', LogSeverity.Warning);
      return;
    }

    this.emitAndLogError(err, err?.response?.data?.message || err?.message);
  };

  private resolveMarketByCoinbaseProductId = (productId: string): Market | undefined => {
    return resolveCoinbaseUsdEquivalentMarket(productId, this.store.marketsById);
  };

  getSupportedTypes = async (): Promise<('spot' | 'cfm' | 'intx')[]> => {
    // In public-only mode, only INTX is available (public endpoint, no auth needed)
    // The /api/v3/brokerage/products endpoint requires JWT and returns ALL products
    // (spot + futures + options = 175k+), so we must not call it in public mode
    if (this.isPublicDataOnly) {
      return ['intx'];
    }

    const supportedTypes: ('spot' | 'cfm' | 'intx')[] = [];
    if (!this.selectedPortfolio) {
      this.selectedPortfolio = await this.findSelectedPortfolio();
    }

    if (this.selectedPortfolio.type === 'INTX') {
      supportedTypes.push('intx');
    } else if (this.selectedPortfolio.type === 'CFM') {
      supportedTypes.push('cfm');
    }
    if (this.supportSpot && this.selectedPortfolio.type !== 'INTX') {
      supportedTypes.push('spot');
    }

    if (!supportedTypes.length) {
      this.emitAndLogError('No supported portfolios found');
    }

    return supportedTypes;
  };

  fetchBalance = async () => {
    if (!this.selectedPortfolio) {
      this.selectedPortfolio = await this.findSelectedPortfolio();
    }

    const supportedTypes = await this.getSupportedTypes();
    if (supportedTypes.includes('intx') && supportedTypes.includes('cfm')) {
      this.emitAndLogError('Cannot have both INTX and CFM portfolios');
      return this.store.balance;
    }

    let baseBalance: Balance = {
      used: 0,
      free: 0,
      total: 0,
      upnl: 0,
    };

    if (supportedTypes.includes('intx')) {
      baseBalance = await this.fetchBalanceIntx();
    }

    if (supportedTypes.includes('cfm')) {
      baseBalance = await this.fetchBalanceCfm();
    }

    if (supportedTypes.includes('spot')) {
      const currencies = await this.fetchSpotCurrencies();
      baseBalance = {
        ...baseBalance,
        currencies,
      };
    }

    return baseBalance;
  };

  fetchSpotCurrencies = async (): Promise<Balance['currencies']> => {
    if (!this.selectedPortfolio || this.selectedPortfolio.type === 'INTX') {
      this.emitAndLogError('No spot portfolio found');
      return this.store.balance.currencies;
    }
    try {
      const { data } = await this.xhr.get<PORTFOLIO_BREAKDOWN_RESPONSE>(ENDPOINTS.PORTFOLIO_BREAKDOWN, {
        params: {
          portfolio_uuid: this.selectedPortfolio.uuid,
        },
      });
      // export type PORTFOLIO_BREAKDOWN_RESPONSE = {
      //   breakdown: {
      //     portfolio: {
      //       name: string;
      //       uuid: string;
      //       type: string;
      //       deleted: boolean;
      //     };
      //     portfolio_balances: {
      //       total_balance: {
      //         value: string;
      //         currency: string;
      //       };
      //       total_futures_balance: {
      //         value: string;
      //         currency: string;
      //       };
      //       total_cash_equivalent_balance: {
      //         value: string;
      //         currency: string;
      //       };
      //       total_crypto_balance: {
      //         value: string;
      //         currency: string;
      //       };
      //       futures_unrealized_pnl: {
      //         value: string;
      //         currency: string;
      //       };
      //       perp_unrealized_pnl: {
      //         value: string;
      //         currency: string;
      //       };
      //     };
      //     spot_positions: {
      //       asset: string;
      //       account_uuid: string;
      //       total_balance_fiat: number;
      //       total_balance_crypto: number;
      //       available_to_trade_fiat: number;
      //       allocation: number;
      //       cost_basis: {
      //         value: string;
      //         currency: string;
      //       };
      //       asset_img_url: string;
      //       is_cash: boolean;
      //       average_entry_price: string;
      //       asset_uuid: string;
      //       available_to_trade_crypto: number;
      //       unrealized_pnl: number;
      //       available_to_transfer_fiat: number;
      //       available_to_transfer_crypto: number;
      //       asset_color: string;
      //     }[];
      //     perp_positions: {
      //       product_id: string;
      //       contract_size: string;
      //       side: 'FUTURES_POSITION_SIDE_LONG' | 'FUTURES_POSITION_SIDE_SHORT';
      //       amount: string;
      //       avg_entry_price: string;
      //       current_price: string;
      //       unrealized_pnl: string;
      //       expiry: string;
      //       underlying_asset: string;
      //       asset_img_url: string;
      //       product_name: string;
      //       venue: string;
      //       notional_value: string;
      //       asset_color: string;
      //     }[];
      //     futures_positions: {
      //       product_id: string;
      //       contract_size: string;
      //       side: 'FUTURES_POSITION_SIDE_LONG' | 'FUTURES_POSITION_SIDE_SHORT';
      //       amount: string;
      //       avg_entry_price: string;
      //       current_price: string;
      //       unrealized_pnl: string;
      //       expiry: string;
      //       underlying_asset: string;
      //       asset_img_url: string;
      //       product_name: string;
      //       venue: string;
      //       notional_value: string;
      //       asset_color: string;
      //     }[];
      //   };
      // };

      const currencies: Balance['currencies'] = data.breakdown.spot_positions.reduce(
        (acc, position) => {
          acc![position.asset] = {
            total: position.available_to_trade_crypto,
            free: position.available_to_trade_crypto,
            used: position.total_balance_crypto - position.available_to_trade_crypto,
            upnl: 0,
          };
          return acc;
        },
        {} as Balance['currencies'],
      );
      return currencies;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.balance.currencies;
    }
  };

  fetchBalanceCfm = async () => {
    try {
      const { data } = await this.xhr.get<BALANCE_RESPONSE_CFM>(ENDPOINTS.BALANCE_CFM);
      // export type BALANCE_RESPONSE_CFM = {
      //   balance_summary: {
      //     futures_buying_power: {
      //       value: string;
      //       currency: string;
      //     };
      //     total_usd_balance: {
      //       value: string;
      //       currency: string;
      //     };
      //     cbi_usd_balance: {
      //       value: string;
      //       currency: string;
      //     };
      //     cfm_usd_balance: {
      //       value: string;
      //       currency: string;
      //     };
      //     total_open_orders_hold_amount: {
      //       value: string;
      //       currency: string;
      //     };
      //     unrealized_pnl: {
      //       value: string;
      //       currency: string;
      //     };
      //     daily_realized_pnl: {
      //       value: string;
      //       currency: string;
      //     };
      //     initial_margin: {
      //       value: string;
      //       currency: string;
      //     };
      //     available_margin: {
      //       value: string;
      //       currency: string;
      //     };
      //     liquidation_threshold: {
      //       value: string;
      //       currency: string;
      //     };
      //     liquidation_buffer_amount: {
      //       value: string;
      //       currency: string;
      //     };
      //     liquidation_buffer_percentage: string;
      //     intraday_margin_window_measure: {
      //       margin_window_type: string;
      //       margin_level: string;
      //       initial_margin: string;
      //       maintenance_margin: string;
      //       liquidation_buffer: string;
      //       total_hold: string;
      //       futures_buying_power: string;
      //     };
      //     overnight_margin_window_measure: {
      //       margin_window_type: string;
      //       margin_level: string;
      //       initial_margin: string;
      //       maintenance_margin: string;
      //       liquidation_buffer: string;
      //       total_hold: string;
      //       futures_buying_power: string;
      //     };
      //   };
      // };

      const total = parseFloat(data.balance_summary.total_usd_balance.value);
      const free = parseFloat(data.balance_summary.available_margin.value);
      const used = total - free;
      return { total, free, used, upnl: 0 };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.balance;
    }
  };

  fetchBalanceIntx = async () => {
    if (!this.selectedPortfolio || this.selectedPortfolio.type !== 'INTX') {
      this.emitAndLogError('No INTX portfolio found');
      return this.store.balance;
    }
    try {
      const { data } = await this.xhr.get<BALANCE_RESPONSE>(ENDPOINTS.BALANCE, {
        params: {
          portfolio_uuid: this.selectedPortfolio!.uuid,
        },
      });

      const summary = data.summary;
      const total = parseFloat(summary.total_balance.value);
      const free = parseFloat(summary.buying_power.value);
      const used = total - free;

      // need to get upnl from positions because the balance version is rebalanced every few seconds and wrong
      const positions = this.store.positions;
      const upnl = positions.reduce((acc, p) => acc + p.unrealizedPnl, 0);

      return { total, free, used, upnl };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.balance;
    }
  };

  private get replayPublicSpot() {
    return Boolean((this.options.extra as { coinbasePublicSpot?: boolean } | undefined)?.coinbasePublicSpot);
  }

  // Declared replay correction: saved spot-only public accounts use the venue's
  // keyless products, paginate fully, and keep polling tickers after startup.
  private fetchReplayPublicSpotProducts = async (): Promise<MARKET_RESPONSE> => {
    const products: MARKET_RESPONSE['products'] = [];
    const cursors = new Set<string>();
    let cursor: string | undefined;
    for (let page = 0; page < 50; page++) {
      const { data } = await this.xhr.get<MARKET_RESPONSE & {
        pagination?: { has_next?: boolean; next_cursor?: string };
      }>('/api/v3/brokerage/market/products', {
        params: { limit: 1000, ...(cursor ? { cursor } : {}) },
      });
      products.push(...data.products.filter((product) => product.product_type === 'SPOT' && !product.future_product_details));
      if (!data.pagination?.has_next) return { products, num_products: products.length };
      cursor = data.pagination.next_cursor;
      if (!cursor || cursors.has(cursor)) throw new Error('Coinbase invalid product cursor');
      cursors.add(cursor);
    }
    throw new Error('Coinbase product page limit');
  };

  private tickReplayPublicSpot = async () => {
    if (this.isDisposed) return;
    try {
      const tickers = this.formatSpotCfmTicker(await this.fetchReplayPublicSpotProducts());
      if (!this.isDisposed) this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
    } finally {
      if (!this.isDisposed) loop(() => this.tickReplayPublicSpot(), this.getTickInterval());
    }
  };

  fetchMarkets = async () => {
    try {
      // In public-only mode, use the public INTX instruments endpoint
      // since /api/v3/brokerage/products requires JWT authentication
      const publicSpot = this.isPublicDataOnly && this.replayPublicSpot;
      if (this.isPublicDataOnly && !publicSpot) {
        return this.fetchMarketsFromIntxInstruments();
      }

      const supportedTypes = publicSpot ? ['spot'] : await this.getSupportedTypes();

      const responses = await Promise.all(
        supportedTypes.map((type) => {
          if (publicSpot) return this.fetchReplayPublicSpotProducts().then((data) => ({ data }));
          return this.xhr.get<MARKET_RESPONSE>(ENDPOINTS.MARKETS, {
            params:
              type === 'spot'
                ? {}
                : {
                    product_type: 'FUTURE',
                    ...(type === 'intx' ? { contract_expiry_type: 'PERPETUAL' } : {}),
                  },
          });
        }),
      );

      const markets: Market[] = [];
      for (const m of responses.flatMap((response) => response.data.products)) {
        const p = m.price_increment;
        const amt = m.base_increment;
        const isSpot = m.future_product_details === undefined;
        const isExpiring = m.future_product_details?.contract_expiry_type === 'EXPIRING';
        const cfmContractSize = isExpiring ? normalizeCfmContractSize(m.future_product_details?.contract_size) : undefined;
        if (isExpiring && cfmContractSize === undefined) {
          this.emitAndLogError(`Coinbase CFM market skipped: invalid contract size for ${m.product_id}`);
          continue;
        }
        const contractSize = cfmContractSize ?? parseFloat(m.future_product_details?.contract_size || '1');
        const base = m.future_product_details?.contract_root_unit || m.base_currency_id || m.product_id.split('-')[0];

        const market: Market = {
          id: m.product_id,
          symbol: m.product_id,
          base,
          quote: m.quote_currency_id,
          active: m.status === 'TRADING',
          isExpiring,
          isSpot,
          precision: {
            amount: parseFloat(amt),
            price: parseFloat(p),
          },
          limits: {
            amount: {
              min: parseFloat(m.base_min_size),
              max: parseFloat(m.base_max_size),
            },
            leverage: {
              min: 1,
              max: isSpot ? 1 : 10,
            },
          },
          contractSize,
          info: m,
        };
        markets.push(market);
      }
      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.markets;
    }
  };

  /**
   * Fetch markets from the public INTX instruments endpoint (no auth needed).
   * Used in public-only mode since /api/v3/brokerage/products requires JWT.
   */
  private fetchMarketsFromIntxInstruments = async (): Promise<Market[]> => {
    try {
      let data: INTX_INSTRUMENTS_RESPONSE = [];
      try {
        const { data: _data } = await this.xhr.get<INTX_INSTRUMENTS_RESPONSE>(ENDPOINTS.INTX_INSTRUMENTS);
        data = _data;
      } catch (err: any) {
        if (err.response?.data?.length) {
          data = err.response.data;
        } else {
          throw err;
        }
      }

      return data
        .filter((m) => m.trading_state === 'TRADING' && m.type === 'PERP')
        .map((m) => ({
          id: `${m.symbol}-INTX`,
          symbol: `${m.symbol}-INTX`,
          base: m.base_asset_name,
          quote: m.quote_asset_name,
          active: true,
          precision: {
            amount: parseFloat(m.base_increment),
            price: parseFloat(m.quote_increment),
          },
          limits: {
            amount: {
              min: parseFloat((m as any).min_quantity || m.min_notional_value || '0.001'),
              max: 1e9,
            },
            leverage: { min: 1, max: 10 },
          },
          contractSize: 1,
          info: m,
        }));
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    const supportedTypes = await this.getSupportedTypes();
    return (
      await Promise.all(
        supportedTypes.map((type) => {
          return type === 'spot'
            ? this.fetchTickersSpot()
            : type === 'cfm'
              ? this.fetchTickersCfm()
              : this.fetchTickersIntx();
        }),
      )
    ).flat();
  };

  fetchTickersSpot = async (): Promise<Ticker[]> => {
    try {
      const { data } = await this.xhr.get<MARKET_RESPONSE>(ENDPOINTS.MARKETS);
      return this.formatSpotCfmTicker(data);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  fetchTickersCfm = async (): Promise<Ticker[]> => {
    try {
      const { data } = await this.xhr.get<MARKET_RESPONSE>(ENDPOINTS.MARKETS, {
        params: {
          product_type: 'FUTURE',
        },
      });
      return this.formatSpotCfmTicker(data);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  formatSpotCfmTicker = (data: MARKET_RESPONSE) => {
    const fakeTickers: Ticker[] = data.products.map((m) => {
      const price = m.price === '' ? 0 : parseFloat(m.price);

      const contractSize = parseFloat(m.future_product_details?.contract_size || '1');

      const rawPricePercentageChange24h = m.price_percentage_change_24h;
      const percentage = rawPricePercentageChange24h === '' ? 0 : parseFloat(rawPricePercentageChange24h);

      const rawVolume = m.volume_24h;
      const volume = rawVolume === '' ? 0 : parseFloat(rawVolume);
      const quoteVolume = round(price * volume * contractSize);

      const rawOpenInterest = m.future_product_details?.perpetual_details?.open_interest;
      const openInterest = rawOpenInterest === '' ? 0 : parseFloat(rawOpenInterest) * contractSize;

      return {
        id: m.product_id,
        symbol: m.product_id,
        bid: price,
        ask: price,
        last: price,
        mark: price,
        index: price,
        percentage,
        fundingRate: 0,
        fundingTime: 0,
        volume,
        quoteVolume,
        openInterest,
      };
    });

    return fakeTickers;
  };

  fetchTickersIntx = async (): Promise<Ticker[]> => {
    try {
      let data: INTX_INSTRUMENTS_RESPONSE = [];

      // dont ask... coinbase api errors but still returns data
      try {
        const { data: _data } = await this.xhr.get<INTX_INSTRUMENTS_RESPONSE>(ENDPOINTS.INTX_INSTRUMENTS);
        data = _data;
      } catch (err: any) {
        if (err.response?.data?.length) {
          data = err.response.data;
        } else {
          throw err;
        }
      }

      const marketMap = this.store.markets.reduce(
        (acc, m) => {
          acc[m.symbol] = m;
          return acc;
        },
        {} as Record<string, Market>,
      );
      const tickers: Ticker[] = data
        .filter((marketIntx) => marketIntx.trading_state === 'TRADING' && marketIntx.type === 'PERP')
        .map((marketIntx) => {
          const quote: INTX_INSTRUMENTS_RESPONSE[0]['quote'] = marketIntx.quote;
          const symbolV3 = `${marketIntx.symbol}-INTX`;
          const rawMarketV3: MARKET_RESPONSE['products'][0] = marketMap[symbolV3]?.info;
          const fundingInterval = parseFloat(marketIntx.funding_interval || '0') / 1000 / 1000;
          return {
            id: symbolV3,
            symbol: symbolV3,
            bid: parseFloat(quote.best_bid_price),
            ask: parseFloat(quote.best_ask_price),
            last: parseFloat(quote.trade_price),
            mark: parseFloat(quote.mark_price),
            index: parseFloat(quote.index_price),
            percentage: parseFloat(rawMarketV3?.price_percentage_change_24h || '0'),
            fundingRate: parseFloat(quote.predicted_funding),
            fundingTime:
              new Date(rawMarketV3?.future_product_details?.perpetual_details?.funding_time).getTime() +
              fundingInterval,
            volume: parseFloat(marketIntx.qty_24hr),
            quoteVolume: parseFloat(marketIntx.notional_24hr),
            openInterest: parseFloat(marketIntx.open_interest),
            info: marketIntx,
          };
        });

      return tickers;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  _fetchTickersV3 = async () => {
    try {
      const tickers: Ticker[] = this.store.markets.map((m) => {
        const info: MARKET_RESPONSE['products'][0] = m.info;
        return {
          id: m.id,
          symbol: m.symbol,
          bid: parseFloat(info.price),
          ask: parseFloat(info.price),
          last: parseFloat(info.price),
          mark: parseFloat(info.price),
          index: parseFloat(info.price),
          percentage: parseFloat(info.price_percentage_change_24h),
          fundingRate: parseFloat(info.future_product_details?.perpetual_details?.funding_rate || '0'),
          fundingTime: parseFloat(info.future_product_details?.perpetual_details?.funding_time || '0'),
          volume: parseFloat(info.volume_24h),
          quoteVolume: 0,
          openInterest: parseFloat(info.future_product_details?.perpetual_details?.open_interest || '0'),
        };
      });
      return tickers;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  fetchPortfolios = async () => {
    try {
      const { data } = await this.xhr.get<LIST_PORTFOLIOS_RESPONSE>(ENDPOINTS.LIST_PORTFOLIOS);
      return data.portfolios;
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      return [];
    }
  };

  findSelectedPortfolio = async () => {
    const portfolios = await this.fetchPortfolios();
    const breakdownResponses = await Promise.all(
      portfolios.map(async (portfolio) => {
        try {
          return await this.xhr.get<PORTFOLIO_BREAKDOWN_RESPONSE>(ENDPOINTS.PORTFOLIO_BREAKDOWN, {
            params: {
              portfolio_uuid: portfolio.uuid,
            },
          });
        } catch {
          return null;
        }
      }),
    );

    const selectedPortfolioIndex = breakdownResponses.findIndex((response) => response !== null);
    if (selectedPortfolioIndex === -1) {
      this.emitAndLogError('No portfolio with permissions found');
      throw new Error('No portfolio with permissions found');
    }
    const foundPortfolio = portfolios[selectedPortfolioIndex];

    // hack convert portfolio to CFM if supported
    if (foundPortfolio.type !== 'INTX') {
      try {
        const { data } = await this.xhr.get<BALANCE_RESPONSE_CFM>(ENDPOINTS.BALANCE_CFM);
        if (data.balance_summary) {
          foundPortfolio.type = 'CFM';
        }
      } catch {
        // do nothing
      }
    }
    return foundPortfolio;
  };

  fetchPositions = async () => {
    const openPositions = await this.fetchOpenPositions();
    const markets = this.store.markets;
    const missingPositionMarkets = markets.filter((m) => !openPositions.find((p) => p.symbol === m.symbol));
    const fakePositions = missingPositionMarkets.map((m) => ({
      id: createPositionId(m.symbol),
      symbol: m.symbol,
      side: PositionSide.Long,
      entryPrice: 0,
      notional: 0,
      leverage: 10,
      positionMode: PositionMode.OneWay,
      marginType: MarginType.Cross,
      unrealizedPnl: 0,
      contracts: 0,
      liquidationPrice: 0,
      info: {},
    }));
    return [...openPositions, ...fakePositions];
  };

  fetchOpenPositions = async () => {
    const supportedTypes = await this.getSupportedTypes();
    return (
      await Promise.all(
        supportedTypes.map((type) => {
          return type === 'spot' ? [] : type === 'cfm' ? this.fetchOpenPositionsCfm() : this.fetchOpenPositionsIntx();
        }),
      )
    ).flat();
  };

  fetchOpenPositionsCfm = async (): Promise<Position[]> => {
    try {
      const { data } = await this.xhr.get<LIST_POSITIONS_CFM_RESPONSE>(ENDPOINTS.LIST_POSITIONS_CFM);

      const formattedPositions: Position[] = [];
      for (const position of data.positions) {
        // export type Position = {
        //   readonly id: string;
        //   readonly symbol: string;
        //   readonly side: PositionSide;
        //   readonly entryPrice: number;
        //   readonly notional: number;
        //   readonly leverage: number;
        //   readonly marginType?: MarginType;
        //   readonly positionMode?: PositionMode;
        //   readonly unrealizedPnl: number;
        //   readonly realizedPnl?: number;
        //   readonly contracts: number;
        //   readonly liquidationPrice: number;
        //   readonly info?: any;
        //   readonly extra?: {
        //     readonly unrealizedPnlNative?: number;
        //     readonly nativeCurrency?: string;
        //   };
        //   readonly lastUpdate?: number;
        // };

        // export type LIST_POSITIONS_CFM_RESPONSE = {
        //   product_id: string;
        //   expiration_time: string;
        //   side: 'LONG' | 'SHORT';
        //   number_of_contracts: string;
        //   current_price: string;
        //   avg_entry_price: string;
        //   unrealized_pnl: string;
        //   daily_realized_pnl: string;
        // }[];

        const market = this.store.marketsById[position.product_id];
        const contractSize = market?.isExpiring ? normalizeCfmContractSize(market.contractSize) : undefined;
        if (contractSize === undefined) {
          this.emitAndLogError(`Coinbase CFM position skipped: missing or invalid contract size for ${position.product_id}`);
          continue;
        }

        const side = position.side === 'LONG' ? PositionSide.Long : PositionSide.Short;
        const entryPrice = parseFloat(position.avg_entry_price);
        const markPrice = parseFloat(position.current_price);
        const size = Math.abs(parseFloat(position.number_of_contracts));
        const unrealisedPnl = parseFloat(position.unrealized_pnl);
        const sizeNotional = size * markPrice * contractSize;
        const sizeInBase = multiply(size, contractSize);
        formattedPositions.push({
          id: createPositionId(position.product_id),
          symbol: position.product_id,
          side,
          entryPrice: entryPrice,
          notional: sizeNotional,
          positionMode: PositionMode.OneWay,
          marginType: MarginType.Cross,
          leverage: 10,
          unrealizedPnl: unrealisedPnl,
          contracts: sizeInBase,
          liquidationPrice: 0,
          extra: {
            contractsNative: size,
          },
          info: position,
        });
      }

      return formattedPositions;
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      return this.store.positions;
    }
  };

  fetchOrders = async (): Promise<Order[]> => {
    try {
      return await this.fetchOpenOrdersSnapshot();
    } catch (err: any) {
      this.emitOrdersFetchError(err);
      throw err;
    }
  };

  private fetchOpenOrdersSnapshot = async (): Promise<Order[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const supportedTypes = await this.getSupportedTypes();
    const requests = supportedTypes.map((type) => ({
      product_type: type === 'spot' ? 'SPOT' : 'FUTURE',
      ...(type === 'cfm' ? { contract_expiry_type: 'EXPIRING' } : {}),
      ...(type === 'intx' ? { contract_expiry_type: 'PERPETUAL' } : {}),
    }));

    const ordersById = new Map<string, Order>();
    // A single poll can produce hundreds of skip messages if a product is
    // missing from the store or if a REST shape is unsupported (common for
    // spot products, in-flight edits, and preview markets). Aggregate the
    // counts by product_id and reason and emit at most one summary line per
    // reason per poll, rather than one error per order per 10s tick.
    const missingMarketByProduct = new Map<string, number>();
    const unsupportedShapeByProduct = new Map<string, number>();

    for (const request of requests) {
      for (const orderStatus of COINBASE_OPEN_ORDER_STATUSES) {
        let cursor: string | undefined;
        do {
          const { data } = await this.xhr.get<LIST_ORDERS_RESPONSE>(ENDPOINTS.LIST_ORDERS, {
            params: {
              ...request,
              order_status: orderStatus,
              limit: COINBASE_ORDER_PAGE_LIMIT,
              ...(cursor ? { cursor } : {}),
            },
          });
          for (const rawOrder of data.orders) {
            const market = this.resolveMarketByCoinbaseProductId(rawOrder.product_id);
            if (!market) {
              missingMarketByProduct.set(
                rawOrder.product_id,
                (missingMarketByProduct.get(rawOrder.product_id) ?? 0) + 1,
              );
              continue;
            }
            const order = mapCoinbaseRestOrder(rawOrder, market, Date.now());
            if (!order) {
              unsupportedShapeByProduct.set(
                rawOrder.product_id,
                (unsupportedShapeByProduct.get(rawOrder.product_id) ?? 0) + 1,
              );
              continue;
            }
            ordersById.set(order.id, order);
          }
          cursor = data.has_next ? data.cursor : undefined;
        } while (cursor);
      }
    }

    if (missingMarketByProduct.size > 0) {
      const summary = Array.from(missingMarketByProduct.entries())
        .map(([productId, count]) => `${productId} (${count})`)
        .join(', ');
      this.emitAndLogError(`Coinbase orders skipped, missing market: ${summary}`);
    }
    if (unsupportedShapeByProduct.size > 0) {
      const summary = Array.from(unsupportedShapeByProduct.entries())
        .map(([productId, count]) => `${productId} (${count})`)
        .join(', ');
      this.emitAndLogError(`Coinbase orders skipped, unsupported REST shape: ${summary}`);
    }

    return Array.from(ordersById.values());
  };

  fetchOpenPositionsIntx = async () => {
    if (!this.selectedPortfolio || this.selectedPortfolio.type !== 'INTX') {
      this.emitAndLogError('No INTX portfolio found');
      return this.store.positions;
    }
    try {
      const { data } = await this.xhr.get<LIST_POSITIONS_INTX_RESPONSE>(ENDPOINTS.LIST_POSITIONS_INTX, {
        params: {
          portfolio_uuid: this.selectedPortfolio.uuid,
        },
      });
      const formattedPositions: Position[] = data.positions.map(
        (position: LIST_POSITIONS_INTX_RESPONSE['positions'][0]) => {
          const side = position.position_side === 'POSITION_SIDE_LONG' ? PositionSide.Long : PositionSide.Short;
          const entryPrice = parseFloat(position.entry_vwap.value);
          const markPrice = parseFloat(position.mark_price.value);
          const size = Math.abs(parseFloat(position.net_size));
          const unrealisedPnl = (side === PositionSide.Long ? 1 : -1) * size * (markPrice - entryPrice);
          const sizeNotional = size * markPrice;
          return {
            id: createPositionId(position.symbol),
            symbol: position.symbol,
            side,
            entryPrice: entryPrice,
            notional: sizeNotional,
            positionMode: PositionMode.OneWay,
            marginType: MarginType.Cross,
            leverage: parseFloat(position.leverage || '10'),
            unrealizedPnl: unrealisedPnl,
            contracts: size,
            liquidationPrice: parseFloat(position.liquidation_price.value),
            info: position,
          };
        },
      );

      return formattedPositions;
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      return this.store.positions;
    }
  };

  dailyCandles: { [key: string]: Candle } = {};

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    // ONE_MINUTE, FIVE_MINUTES, FIFTEEN_MINUTES, THIRTY_MINUTES, ONE_HOUR, TWO_HOURS, SIX_HOURS, ONE_DAY
    const intervalMap: Partial<Record<Timeframe, string>> = {
      '1m': 'ONE_MINUTE',
      '5m': 'FIVE_MINUTE',
      '15m': 'FIFTEEN_MINUTE',
      '30m': 'THIRTY_MINUTE',
      '1h': 'ONE_HOUR',
      '2h': 'TWO_HOUR',
      '6h': 'SIX_HOUR',
      '1d': 'ONE_DAY',
    };
    const granularity = intervalMap[interval];
    if (!granularity) {
      throw new Error(`Invalid interval ${interval}`);
    }
    const limit = Math.min(opts.limit || 300, 300);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.xhr.get<CANDLES_RESPONSE>(ENDPOINTS.KLINE, {
      params: {
        product_id: normalizeCoinbaseUsdEquivalentProductId(opts.symbol),
        granularity,
        start: Math.round(start.valueOf() / 1000),
        end: Math.round(end.valueOf() / 1000),
      },
    });

    const candles: Candle[] = data.candles
      .map(({ start, low, high, open, close, volume }) => {
        const formattedCandle = {
          timestamp: +start,
          open: parseFloat(open),
          high: parseFloat(high),
          low: parseFloat(low),
          close: parseFloat(close),
          volume: parseFloat(volume),
        };
        if (interval === '1d') {
          if (!this.dailyCandles[opts.symbol] || this.dailyCandles[opts.symbol].timestamp < formattedCandle.timestamp) {
            this.dailyCandles[opts.symbol] = formattedCandle;
          }
        }
        return formattedCandle;
      })
      .reverse();

    return candles;
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  cancelOrders = async (orders: Order[]) => {
    try {
      const lots = chunk(
        orders.map((o) => o.id),
        100,
      );
      await forEachSeries(lots, async (lot) => {
        await this.xhr.post(ENDPOINTS.CANCEL_ORDER_BATCH, {
          order_ids: lot,
        });

        this.store.updateOrdersStatus(
          lot.map((id) => ({ id })),
          OrderStatus.Canceled,
        );
      });
      this.scheduleOpenOrdersRestReconciliation('cancel-order');
      this.schedulePrivateUserStreamWriteCheck();
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    // INTX remains on the legacy lifecycle until the Starbase cutover. Do not
    // broaden this branch: non-INTX orders are native-edit-or-fail-closed.
    if (this.selectedPortfolio?.type === 'INTX') {
      const newOrder = {
        symbol: order.symbol,
        type: order.type,
        side: order.side,
        price: order.price,
        amount: order.amount,
        reduceOnly: order.reduceOnly || false,
      };

      if ('price' in update) newOrder.price = update.price;
      if ('amount' in update) newOrder.amount = update.amount;

      await this.cancelOrders([order]);
      return await this.placeOrder(newOrder);
    }

    const rejectPreflight = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };
    const canonicalOrder = this.store.orders.find(({ id }) => id === order.id);
    if (!canonicalOrder || canonicalOrder.symbol !== order.symbol) {
      return rejectPreflight(`Coinbase cannot safely edit ${order.id}: canonical open order is not reconciled`);
    }

    const market = this.store.marketsBySymbol[canonicalOrder.symbol];
    if (!market) {
      return rejectPreflight(`Coinbase cannot safely edit ${order.id}: market ${canonicalOrder.symbol} is not reconciled`);
    }
    const marketInfo = market.info as Partial<MARKET_RESPONSE['products'][number]> | undefined;
    const isProvenSpot =
      market.isSpot === true &&
      market.isExpiring !== true &&
      market.contractSize === 1 &&
      marketInfo?.product_type === 'SPOT' &&
      marketInfo.future_product_details === undefined;
    if (!isProvenSpot) {
      const unit = market.isExpiring || marketInfo?.product_type === 'FUTURE' ? 'CFM contract' : 'unknown';
      return rejectPreflight(
        `Coinbase cannot safely edit ${order.id}: ${unit} size units are not proven for native edit`,
      );
    }

    const raw = canonicalOrder.info as Record<string, any> | undefined;
    const rawLimitConfig =
      raw?.order_configuration?.limit_limit_gtc ||
      raw?.order_configuration?.limit_limit_gtd ||
      raw?.order_configuration?.limit_limit_fok ||
      raw?.order_configuration?.sor_limit_ioc;
    const rawOrderId = raw?.order_id;
    const rawProductId = raw?.product_id;
    const rawStatus = raw?.status;
    const rawType = raw?.order_type;
    const rawSide = raw?.order_side ?? raw?.side;
    const expectedSide = canonicalOrder.side === OrderSide.Buy ? 'BUY' : 'SELL';
    const rawStopPrice = raw?.stop_price ?? '';
    const rawTypeIsOrdinaryLimit = rawType === 'Limit' || rawType === 'LIMIT';
    const hasNoTrigger =
      typeof rawStopPrice === 'string' && (rawStopPrice === '' || isCoinbaseZeroDecimal(rawStopPrice));
    const rawProductMatchesMarket =
      typeof rawProductId === 'string' &&
      (rawProductId === market.id ||
        normalizeCoinbaseUsdEquivalentProductId(rawProductId) === normalizeCoinbaseUsdEquivalentProductId(market.id));
    if (
      canonicalOrder.status !== OrderStatus.Open ||
      canonicalOrder.type !== OrderType.Limit ||
      typeof rawOrderId !== 'string' ||
      rawOrderId !== canonicalOrder.id ||
      !rawProductMatchesMarket ||
      typeof rawStatus !== 'string' ||
      rawStatus !== 'OPEN' ||
      typeof rawType !== 'string' ||
      !rawTypeIsOrdinaryLimit ||
      typeof rawSide !== 'string' ||
      rawSide !== expectedSide ||
      !hasNoTrigger ||
      canonicalOrder.takeProfit !== undefined ||
      canonicalOrder.stopLoss !== undefined ||
      canonicalOrder.close === true ||
      canonicalOrder.reduceOnly
    ) {
      return rejectPreflight(
        'Coinbase native edit supports only exactly reconciled OPEN zero-fill ordinary spot LIMIT orders',
      );
    }

    const rawFilled = raw?.cumulative_quantity ?? raw?.filled_size;
    const rawRemaining = raw?.leaves_quantity ?? rawLimitConfig?.base_size;
    const rawPrice = raw?.limit_price ?? rawLimitConfig?.limit_price;
    const currentAmount = adjust(canonicalOrder.amount, market.precision.amount);
    const currentRemaining = adjust(canonicalOrder.remaining, market.precision.amount);
    const currentPrice = adjust(canonicalOrder.price, market.precision.price);
    if (
      !isCoinbaseZeroDecimal(rawFilled) ||
      !isCoinbaseUnsignedDecimal(rawRemaining) ||
      !new BigNumber(rawRemaining).isGreaterThan(0) ||
      !isCoinbaseUnsignedDecimal(rawPrice) ||
      !new BigNumber(rawPrice).isGreaterThan(0) ||
      canonicalOrder.filled !== 0 ||
      currentAmount !== canonicalOrder.amount ||
      currentRemaining !== canonicalOrder.remaining ||
      currentPrice !== canonicalOrder.price ||
      !new BigNumber(rawRemaining).isEqualTo(String(currentAmount)) ||
      currentRemaining !== currentAmount ||
      !new BigNumber(rawPrice).isEqualTo(String(currentPrice))
    ) {
      return rejectPreflight(
        `Coinbase cannot safely edit ${order.id}: full current spot price and zero-fill base size are not reconciled`,
      );
    }

    const hasAmountUpdate = 'amount' in update;
    const hasPriceUpdate = 'price' in update;
    if (hasAmountUpdate === hasPriceUpdate) {
      return rejectPreflight('Coinbase native edit requires exactly one amount or price update');
    }
    const amendedAmount = adjust(hasAmountUpdate ? update.amount : currentAmount, market.precision.amount);
    const amendedPrice = adjust(hasPriceUpdate ? update.price : currentPrice, market.precision.price);
    if (
      !Number.isFinite(amendedAmount) ||
      amendedAmount <= 0 ||
      amendedAmount < market.limits.amount.min ||
      amendedAmount > market.limits.amount.max
    ) {
      return rejectPreflight('Coinbase amended spot size is outside the reconciled market limits');
    }
    if (!Number.isFinite(amendedPrice) || amendedPrice <= 0) {
      return rejectPreflight('Coinbase amended spot price must be greater than zero');
    }
    if (amendedAmount === currentAmount && amendedPrice === currentPrice) {
      return rejectPreflight('Coinbase native edit must change price or size after market-precision normalization');
    }

    const payload: EDIT_ORDER_REQUEST = {
      order_id: canonicalOrder.id,
      price: String(amendedPrice),
      size: String(amendedAmount),
    };
    try {
      const { data } = await this.xhr.post<EDIT_ORDER_RESPONSE>(ENDPOINTS.EDIT_ORDER, payload);
      const errors = Array.isArray(data?.errors) ? data.errors : undefined;
      if (data?.success !== true || errors === undefined || errors.length > 0) {
        const reasons = (errors || []).flatMap(({ edit_failure_reason, preview_failure_reason }) =>
          [edit_failure_reason, preview_failure_reason].filter((reason): reason is string => Boolean(reason)),
        );
        throw new Error(
          reasons.join(', ') ||
            (errors === undefined
              ? 'Coinbase edit-order response did not include the documented errors array'
              : 'Coinbase edit order failed'),
        );
      }
      this.scheduleOpenOrdersRestReconciliation('edit-order');
      this.schedulePrivateUserStreamWriteCheck();
      return [canonicalOrder.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message || String(err));
      throw err;
    }
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    const payload = this.formatCreateOrder(opts);
    try {
      const { data } = await this.xhr.post<PLACE_ORDER_RESPONSE>(ENDPOINTS.ORDER, payload);

      if (!data.success) {
        throw new Error(
          data.error_response?.new_order_failure_reason ||
            data.error_response?.preview_failure_reason ||
            data.failure_reason ||
            'Unknown error',
        );
      }
      const orderId = data.success_response?.order_id ?? data.order_id;
      if (!orderId) {
        throw new Error('Coinbase create-order response did not include an order ID');
      }
      const optimisticOrder = this.buildOptimisticOrderFromCreate(payload, orderId, data);
      if (optimisticOrder) {
        this.store.addOrUpdateOrder(optimisticOrder);
      }
      this.scheduleOpenOrdersRestReconciliation('place-order');
      this.schedulePrivateUserStreamWriteCheck();
      return [orderId];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw new Error(err?.response?.data?.message || err?.message);
    }
  };

  private formatCreateOrder = (opts: PlaceOrderOpts): PLACE_ORDER_REQUEST => {
    // if (opts.type === OrderType.TrailingStopLoss) {
    //   return this.formatCreateTrailingStopLossOrder(opts);
    // }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    // const pSide = this.getOrderPositionSide(opts);
    //
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);
    //
    // // We use price only for limit orders
    // // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;
    //
    // // Binance stopPrice only for SL or TP orders
    // const priceField = isStopOrTP ? 'stopPrice' : 'price';
    //
    // const reduceOnly = opts.reduceOnly;
    // const timeInForce = opts.timeInForce
    //   ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
    //   : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    // type ORDER_REQUEST_CONFIG = {
    //   market_market_ioc?: {
    //     quote_size: string;
    //     base_size: string;
    //   };
    //   sor_limit_ioc?: {
    //     base_size: string;
    //     limit_price: string;
    //   };
    //   limit_limit_gtc?: {
    //     base_size: string;
    //     limit_price: string;
    //     post_only?: boolean;
    //   };
    //   limit_limit_gtd?: {
    //     base_size: string;
    //     limit_price: string;
    //     end_time: string; // RFC3339 Timestamp
    //     post_only?: boolean;
    //   };
    //   stop_limit_stop_limit_gtc?: {
    //     base_size: string;
    //     limit_price: string;
    //     stop_price: string;
    //     stop_direction: STOP_DIRECTION;
    //   };
    //   stop_limit_stop_limit_gtd?: {
    //     base_size: string;
    //     limit_price: string;
    //     stop_price: string;
    //     end_time: string; // RFC3339 Timestamp
    //     stop_direction: STOP_DIRECTION;
    //   };
    //   trigger_bracket_gtc?: {
    //     base_size: string;
    //     limit_price: string;
    //     stop_trigger_price: string;
    //   };
    //   trigger_bracket_gtd?: {
    //     base_size: string;
    //     limit_price: string;
    //     stop_trigger_price: string;
    //     end_time: string; // RFC3339 Timestamp
    //   };
    // };
    //
    // export type PLACE_ORDER_REQUEST = {
    //   client_order_id: string;
    //   product_id: string;
    //   side: 'BUY' | 'SELL';
    //   order_configuration: ORDER_REQUEST_CONFIG;
    //   self_trade_prevention_id?: string;
    //   leverage?: string;
    //   margin_type?: MARGIN_TYPE;
    //   retail_portfolio_id?: string;
    // };

    let order_configuration: PLACE_ORDER_REQUEST['order_configuration'];
    if (opts.type === OrderType.Market) {
      order_configuration = {
        market_market_ioc: {
          base_size: `${amount}`,
        },
      };
    } else if (
      opts.type === OrderType.StopLoss
      // || opts.type === OrderType.TakeProfit
    ) {
      const stopIsAbove = opts.side === OrderSide.Buy && opts.type === OrderType.StopLoss;
      // || (opts.side === OrderSide.Sell && opts.type === OrderType.TakeProfit);
      const stopPrice = price || 0;
      const limitPrice = adjust(stopPrice * (stopIsAbove ? 1.01 : 0.99), pPrice);
      order_configuration = {
        stop_limit_stop_limit_gtc: {
          base_size: `${amount}`,
          limit_price: `${limitPrice}`,
          stop_price: `${stopPrice}`,
          stop_direction: stopIsAbove ? 'STOP_DIRECTION_STOP_UP' : 'STOP_DIRECTION_STOP_DOWN',
        },
      };
    } else {
      order_configuration = {
        limit_limit_gtc: {
          base_size: `${amount}`,
          limit_price: `${price}`,
          post_only: opts.timeInForce === OrderTimeInForce.PostOnly,
        },
      };
    }

    const req: PLACE_ORDER_REQUEST = {
      client_order_id: this.resolveCallerClientOrderId(opts) ?? 'tealstreet-' + uuid(),
      product_id: opts.symbol,
      side: opts.side === OrderSide.Buy ? 'BUY' : 'SELL',
      // TODO. leverage setting?
      leverage: '10',
      order_configuration,
    };

    return req;

    // const req = omitUndefined({
    //   symbol: opts.symbol,
    //   positionSide: pSide,
    //   side: inverseObj(ORDER_SIDE)[opts.side],
    //   type: inverseObj(ORDER_TYPE)[opts.type],
    //   quantity: amount ? `${amount}` : undefined,
    //   [priceField]: price ? `${price}` : undefined,
    //   timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
    //   closePosition: isStopOrTP ? 'true' : undefined,
    //   reduceOnly: reduceOnly && !isStopOrTP ? 'true' : undefined,
    // });
    //
    // const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    // const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;
    //
    // const lotSize = adjust((amount - rest) / lots, pAmount);
    //
    // const payloads: Array<Record<string, any>> = times(lots, () => ({
    //   ...req,
    //   quantity: `${lotSize}`,
    // }));
    //
    // if (rest) {
    //   payloads.push({ ...req, quantity: `${rest}` });
    // }
    //
    // if (opts.stopLoss) {
    //   payloads.push({
    //     ...omit(req, 'price'),
    //     side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
    //     type: inverseObj(ORDER_TYPE)[OrderType.StopLoss],
    //     stopPrice: `${opts.stopLoss}`,
    //     timeInForce: 'GTC',
    //     closePosition: 'true',
    //   });
    // }
    //
    // if (opts.takeProfit) {
    //   payloads.push({
    //     ...omit(req, 'price'),
    //     side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
    //     type: inverseObj(ORDER_TYPE)[OrderType.TakeProfit],
    //     stopPrice: `${opts.takeProfit}`,
    //     timeInForce: 'GTC',
    //     closePosition: 'true',
    //   });
    // }
    //
    // // We need to set orderId for each order
    // // otherwise Binance will duplicate the IDs
    // // when its sent in batches
    // for (const payload of payloads) {
    //   payload.newClientOrderId = 'x-s3vFFRQp' + uuid();
    // }
    //
    // return payloads;
  };
}
