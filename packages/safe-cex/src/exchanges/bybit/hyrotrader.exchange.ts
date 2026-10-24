import type { IncomeHistoryOptions, IncomeRecord, Order, PlaceOrderOpts } from '../../types';
import type { BybitCreateOrderResponse } from './bybit.exchange';
import type { BybitPlaceOrderOpts } from './bybit.order-params';

import { PositionMode } from '../../types';
import { markBaseCapabilityFallback } from '../base';
import { BybitExchange } from './bybit.exchange';
import { getBybitResponseMeta } from './bybit.response';
import {
  ENDPOINTS,
  HYROTRADER_PROD_URL,
  HYROTRADER_PROD_WS_URL,
  HYROTRADER_SIMULATOR_URL,
  HYROTRADER_SIMULATOR_WS_URL,
} from './bybit.types';
import { HyroOrderRecovery, hyroSubmissionKey } from './hyrotrader.order-recovery';
import { HyroPrivateWebsocket } from './hyrotrader.ws-private';

export class HyrotraderExchange extends BybitExchange {
  // Order recovery re-mints orderLinkId after a reject, so a caller ID could be rewritten.
  protected readonly clientOrderIdRule = undefined;
  isDemo = false;
  positionMode = PositionMode.OneWay;
  privateWebsocket = new HyroPrivateWebsocket(this);
  private snapshotVersion = 0;
  private ordersSnapshotVersion = 0;
  private activeOrderObservations = new Set<Set<string>>();
  private orderRecovery = new HyroOrderRecovery(
    () => this.xhr,
    () => this.isDisposed,
    () => this.assertPrivateReady(),
    () => `${this.privateWebsocket.sessionVersion}:${this.ordersSnapshotVersion}`,
  );
  private pendingOrderLists = new Map<string, { index: number; ids: string[]; running?: Promise<string[]> }>();

  private fail = (message: string): never => {
    this.emitAndLogError(message);
    throw new Error(message);
  };

  private isFiniteNumeric(value: unknown) {
    return (
      (typeof value === 'number' || typeof value === 'string') &&
      String(value).trim() !== '' &&
      Number.isFinite(Number(value))
    );
  }

  private isFlatPositionRow(row: Record<string, any>) {
    return this.isFiniteNumeric(row.size) && Number(row.size) === 0;
  }

  fetchRawBalance = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.BALANCE, { params: { accountType: 'UNIFIED' } });
    const status = getBybitResponseMeta(data);
    if (!status.isSuccess) this.fail(status.retMsg || 'HyroTrader wallet request failed');
    const account = data?.result?.list?.[0];
    const values = [account?.totalEquity, account?.totalWalletBalance];
    const hasBalance = values.some(
      (value) => value !== undefined && value !== null && value !== '' && Number.isFinite(Number(value)),
    );
    if (!Array.isArray(data?.result?.list) || account?.accountType !== 'UNIFIED' || !hasBalance) {
      this.fail('HyroTrader wallet response is missing the UNIFIED account');
    }
    const finiteFields = (row: Record<string, any>, fields: string[]) =>
      fields.every((field) => row[field] === undefined || row[field] === '' || this.isFiniteNumeric(row[field]));
    if (
      !finiteFields(account, [
        'totalEquity',
        'totalWalletBalance',
        'totalMarginBalance',
        'totalAvailableBalance',
        'totalInitialMargin',
        'totalMaintenanceMargin',
      ]) ||
      (account.coin !== undefined &&
        (!Array.isArray(account.coin) ||
          account.coin.some(
            (coin: any) =>
              !coin ||
              typeof coin.coin !== 'string' ||
              !finiteFields(coin, [
                'equity',
                'walletBalance',
                'usdValue',
                'totalPositionIM',
                'totalOrderIM',
                'locked',
                'bonus',
              ]),
          )))
    ) {
      this.fail('HyroTrader wallet response contains invalid monetary fields');
    }
    return data.result.list;
  };

  validateAccount = async () => {
    try {
      return { error: '', balance: await this.fetchBalance() };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.emitAndLogError(message);
      return { error: message };
    }
  };

  fetchMarginAccountInfos = async () => {
    const balance = await this.fetchBalance();
    if (!this.isDisposed) this.store.update({ balance });
  };

  private assertPrivateListResponse(data: any, endpoint: string) {
    const status = getBybitResponseMeta(data);
    if (!status.isSuccess) this.fail(status.retMsg || `HyroTrader ${endpoint} request failed`);
    if (!Array.isArray(data?.result?.list)) this.fail(`HyroTrader ${endpoint} response has no list`);
    for (const row of data.result.list) {
      const valid =
        row &&
        typeof row.symbol === 'string' &&
        row.symbol.endsWith('USDT') &&
        (endpoint === ENDPOINTS.POSITIONS
          ? row.size !== undefined &&
            row.size !== '' &&
            this.isFiniteNumeric(row.size) &&
            Number(row.size) >= 0 &&
            Number(row.positionIdx ?? 0) === 0
          : typeof row.orderId === 'string' && row.orderId.length > 0);
      if (!valid) this.fail(`HyroTrader ${endpoint} returned a malformed row`);
      if (endpoint === ENDPOINTS.POSITIONS) {
        const isFlatPosition = this.isFlatPositionRow(row);
        if (!this.isFiniteNumeric(row.leverage) || Number(row.leverage) <= 0) {
          if (!isFlatPosition) this.fail('HyroTrader position response is missing valid leverage');
        }
        const numericFields = [
          'avgPrice',
          'entryPrice',
          'markPrice',
          'unrealisedPnl',
          'leverage',
          'positionValue',
          'positionIM',
          'curRealisedPnl',
          'liqPrice',
        ];
        if (
          numericFields.some(
            (field) => row[field] !== undefined && row[field] !== '' && !this.isFiniteNumeric(row[field]),
          )
        ) {
          this.fail('HyroTrader position response contains an invalid numeric value');
        }
      }
      if (endpoint === ENDPOINTS.POSITIONS && Number(row.size) > 0) {
        const hasEntryPrice =
          (row.avgPrice !== undefined && row.avgPrice !== '' && this.isFiniteNumeric(row.avgPrice)) ||
          (row.entryPrice !== undefined && row.entryPrice !== '' && this.isFiniteNumeric(row.entryPrice));
        if (
          !['Buy', 'Sell'].includes(row.side) ||
          !hasEntryPrice ||
          ['markPrice', 'unrealisedPnl'].some(
            (field) => row[field] === undefined || row[field] === '' || !this.isFiniteNumeric(row[field]),
          )
        )
          this.fail('HyroTrader position response is missing side, price or unrealized PnL');
      }
    }
  }

  mapPosition(row: Record<string, any>) {
    const normalizedRow =
      this.isFlatPositionRow(row) && (!this.isFiniteNumeric(row.leverage) || Number(row.leverage) <= 0)
        ? { ...row, leverage: this.store.positions.find((position) => position.symbol === row.symbol)?.leverage ?? 1 }
        : row;
    this.assertPrivateListResponse({ retCode: 0, retMsg: 'OK', result: { list: [normalizedRow] } }, ENDPOINTS.POSITIONS);
    const position = super.mapPosition({ ...normalizedRow, positionIdx: 0 });
    const fields = [
      position.entryPrice,
      position.contracts,
      position.notional,
      position.leverage,
      position.unrealizedPnl,
      position.realizedPnl,
      position.liquidationPrice,
    ];
    if (
      fields.some((value) => !Number.isFinite(value)) ||
      (position.marginSize !== undefined && !Number.isFinite(position.marginSize))
    ) {
      this.fail('HyroTrader position mapping contains invalid numeric fields');
    }
    return position;
  }

  fetchOrders = async () => this.fetchOrdersHelper('linear', 'USDT');

  private async fetchPrivateRows(endpoint: string, params: Record<string, unknown>) {
    const rows: Record<string, any>[] = [];
    const seen = new Set<string>();
    let cursor = '';
    for (let page = 0; page < 100; page++) {
      const { data } = await this.xhr.get(endpoint, { params: { ...params, limit: 50, cursor } });
      this.assertPrivateListResponse(data, endpoint);
      rows.push(...data.result.list);
      const next = data.result.nextPageCursor;
      if (next === undefined || next === '') return rows;
      if (typeof next !== 'string' || seen.has(next)) this.fail('HyroTrader private snapshot cursor did not advance');
      seen.add(next);
      cursor = next;
    }
    return this.fail('HyroTrader private snapshot page limit exceeded');
  }

  fetchOrdersHelper = async (category: string, settleCoin?: string) => {
    const rows = await this.fetchPrivateRows(ENDPOINTS.UNFILLED_ORDERS, { category, settleCoin });
    return this.mapValidatedOrders(rows);
  };

  private mapValidatedOrders(rows: Record<string, any>[]) {
    this.assertPrivateListResponse({ retCode: 0, retMsg: 'OK', result: { list: rows } }, ENDPOINTS.UNFILLED_ORDERS);
    const orders = rows.flatMap((row) => this.mapOrder(row));
    if (
      orders.some(
        (order) =>
          !order.status ||
          !order.type ||
          !order.side ||
          [order.price, order.amount, order.remaining, order.filled].some(
            (value) => !Number.isFinite(value) || value < 0,
          ),
      )
    ) {
      this.fail('HyroTrader order response contains incomplete or invalid order fields');
    }
    return orders;
  }

  fetchPositionsHelper = async (symbols?: string[], category = 'linear', settleCoin = 'USDT') => {
    const rows: Record<string, any>[] = [];
    for (const symbol of symbols?.length ? symbols : [undefined]) {
      rows.push(
        ...(await this.fetchPrivateRows(ENDPOINTS.POSITIONS, { category, ...(symbol ? { symbol } : { settleCoin }) })),
      );
    }
    return this.dedupePositionsById(rows.map((row) => this.mapPosition(row)));
  };

  reconcilePrivateState = async (isCurrent: () => boolean = () => true, includeOrders = true): Promise<boolean> => {
    const version = ++this.snapshotVersion;
    const revision = this.privateWebsocket.stateRevision;
    try {
      const [orders, positions, rawBalance] = await Promise.all([
        includeOrders ? this.fetchOrders() : Promise.resolve(undefined),
        this.fetchPositions(),
        this.fetchRawBalance(),
      ]);
      if (
        this.isDisposed ||
        !isCurrent() ||
        version !== this.snapshotVersion ||
        revision !== this.privateWebsocket.stateRevision
      )
        return false;
      const formatted = this.formatBalance(rawBalance);
      const upnl = positions.reduce((sum, position) => sum + position.unrealizedPnl, 0);
      this.store.update({
        ...(orders ? { orders } : {}),
        positions,
        balance: { ...formatted, total: formatted.total + formatted.upnl - upnl, upnl },
        loaded: { ...this.store.loaded, ...(orders ? { orders: true } : {}), positions: true, balance: true },
      });
      if (orders) this.ordersSnapshotVersion++;
      return true;
    } catch (error) {
      this.emitAndLogError(error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  };

  fetchAndSetPrivateSnapshot = async () => {
    await this.reconcilePrivateState();
  };
  fetchOrdersPositionsAndBalance = async () => {
    await this.reconcilePrivateState();
  };
  fetchAndSetPrivateSnapshotHelper = async (includeOrders = true) => {
    if (this.privateWebsocket.started && !this.privateWebsocket.ready) return;
    await this.reconcilePrivateState(() => true, includeOrders);
  };

  protected fetchAndCacheApiKeyInfo = async () => {};

  fetchApiKeyInfo = async (): Promise<never> => this.fail('HyroTrader does not provide API-key metadata');
  getAccount = async (): Promise<never> => this.fail('HyroTrader does not provide account identity metadata');
  fetchFees = async (_category: string) => [];
  fetchMarkets = async () => {
    const markets = await this.fetchMarketsHelper('linear');
    return [
      ...new Map(markets.filter((market) => market.quote === 'USDT').map((market) => [market.symbol, market])).values(),
    ];
  };
  fetchPositionMode = async (_symbol: string) => PositionMode.OneWay;

  changePositionMode = async (hedged: boolean, _symbol?: string) => {
    if (hedged) this.fail('HyroTrader supports one-way position mode only');
  };

  changeMarginType = async (isolated: boolean) => {
    if (isolated) this.fail('HyroTrader supports cross margin only');
  };

  fetchIncomeHistory = markBaseCapabilityFallback(
    'fetchIncomeHistory',
    async (_opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => [],
  );
  fetchIncomeHistoryRange = markBaseCapabilityFallback(
    'fetchIncomeHistoryRange',
    async (_startTime: number, _endTime: number, _incomeType?: string): Promise<IncomeRecord[]> => [],
  );

  private assertPrivateReady() {
    if (this.privateWebsocket.started && !this.privateWebsocket.ready) {
      this.fail('HyroTrader account is reconnecting; wait for private state reconciliation before placing orders');
    }
  }

  protected validateOrderCapabilities(opts: BybitPlaceOrderOpts) {
    this.assertPrivateReady();
    if (this.store.marketsBySymbol[opts.symbol]?.quote !== 'USDT') {
      this.fail('HyroTrader supports USDT perpetual instruments only');
    }
    if (opts.maxSlippagePct !== undefined) {
      this.fail(
        'HyroTrader does not support client-defined slippage limits; protection uses the simulator-configured gate',
      );
    }
    if ('bybitBbo' in opts || 'bybitSlippageToleranceTicks' in opts) {
      this.fail('HyroTrader does not support Bybit BBO or TickSize slippage controls');
    }
  }

  placeOrders = async (orders: BybitPlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    orders.forEach((order) => this.validateOrderCapabilities(order));
    const key = hyroSubmissionKey(orders);
    let pending = this.pendingOrderLists.get(key);
    if (pending?.running) return pending.running;
    if (!pending) {
      pending = { index: 0, ids: [] };
      this.pendingOrderLists.set(key, pending);
    }
    const current = pending;
    current.running = (async () => {
      for (; current.index < orders.length; current.index++) {
        current.ids.push(...(await this.placeOrder(orders[current.index])));
      }
      return current.ids;
    })();
    try {
      const ids = await current.running;
      this.pendingOrderLists.delete(key);
      return ids;
    } finally {
      current.running = undefined;
    }
  };

  observePrivateOrders(rows: Record<string, any>[]) {
    this.orderRecovery.observeOrders(rows);
    for (const row of rows) {
      if (typeof row.orderId === 'string') {
        for (const observations of this.activeOrderObservations) observations.add(row.orderId);
      }
    }
  }

  protected async submitCreateOrders(payloads: Record<string, any>[], opts: PlaceOrderOpts) {
    const observations = new Set<string>();
    this.activeOrderObservations.add(observations);
    try {
      const normalizedPayloads = payloads.map((payload) => {
        const triggerDirection = payload.triggerDirection;
        if (typeof triggerDirection !== 'string') return payload;
        const numericTriggerDirection = Number(triggerDirection);
        if (!Number.isInteger(numericTriggerDirection)) return payload;
        return { ...payload, triggerDirection: numericTriggerDirection };
      });
      return await this.orderRecovery.submit(normalizedPayloads, (response: BybitCreateOrderResponse) => {
        response.recorded = true;
        const id = response.data.result.orderId;
        if (this.isDisposed || observations.has(id) || response.authoritativeSeen) return;
        if (response.terminal) {
          this.store.removeOrders([{ id }, { id: `${id}__stop_loss` }, { id: `${id}__take_profit` }]);
        } else if (response.recoveredOrder) {
          this.store.addOrUpdateOrders(this.mapValidatedOrders([response.recoveredOrder]));
        } else if (!response.reconciled && !this.store.orders.some((order) => order.id === id)) {
          this.recordRecentlyPlacedOrderFromOpts(id, opts, response.payload);
        }
      });
    } catch (error) {
      this.emitAndLogError(error instanceof Error ? error : new Error(String(error)));
      throw error;
    } finally {
      this.activeOrderObservations.delete(observations);
    }
  }

  // Hyro's simulator wraps the same "already gone" semantic as live Bybit in
  // different phrasings ("Order does not exist or is no longer open" alongside
  // the Bybit-live "order not exists" / "too late to cancel"). Keep both sets
  // accepted so chase-cancel races never surface as user-visible errors from
  // the single-cancel path or from inherited bulk/cancel-all paths.
  protected isAlreadyGoneCancelMessage(message: unknown): boolean {
    if (super.isAlreadyGoneCancelMessage(message)) return true;
    const normalized = `${message || ''}`.toLowerCase();
    return normalized.includes('does not exist') || normalized.includes('no longer open');
  }

  cancelOrdersHelper = async (orders: Order[], category: string) => {
    for (const order of orders) {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.CANCEL_ORDER, {
          category,
          symbol: order.symbol,
          orderId: order.id,
        });
        const status = getBybitResponseMeta(data);
        if (!status.isSuccess && !this.isAlreadyGoneCancelMessage(status.retMsg)) {
          this.fail(status.retMsg || 'HyroTrader cancellation failed');
        }
        this.retireCancelledOrders([
          order,
          ...this.store.orders.filter(
            (row) => row.id === `${order.id}__stop_loss` || row.id === `${order.id}__take_profit`,
          ),
        ]);
      } catch (error) {
        this.emitAndLogError(error instanceof Error ? error : new Error(String(error)));
        throw error;
      }
    }
  };

  constructor(...args: ConstructorParameters<typeof BybitExchange>) {
    super(...args);
    this.isDemo = this.hyrotraderMode() === 'demo';
    const disposeBybit = this.dispose;
    this.dispose = () => {
      this.orderRecovery.dispose();
      this.activeOrderObservations.clear();
      this.pendingOrderLists.clear();
      disposeBybit();
    };
  }

  get supportsTradeDerivedCandles() {
    return false;
  }

  // Hyro's public WS rejects oversized subscribe batches with
  // "at most 32 topics per connection". Keep bulk kline subscribes well below
  // that cap; the base Bybit livenet default of 50 would get the connection
  // closed by the simulator.
  get publicKlineSubscribeChunkSize() {
    return 10;
  }

  get supportedCategories() {
    return ['linear'];
  }

  private hyrotraderMode() {
    return this.options.extra?.hyrotraderMode === 'demo' ? 'demo' : 'production';
  }

  private getConfiguredUrls() {
    const simulator = {
      label: 'simulator',
      rest: HYROTRADER_SIMULATOR_URL,
      ws: HYROTRADER_SIMULATOR_WS_URL,
    };
    if (this.hyrotraderMode() === 'demo') return simulator;

    const production = {
      label: 'production',
      rest: HYROTRADER_PROD_URL,
      ws: HYROTRADER_PROD_WS_URL,
    };
    if (production.rest.livenet || production.rest.testnet) return production;

    // V1 user-pasted challenge keys are still simulator-backed until Hyro's
    // production HTTPS/WSS origin is available. Keep the account classified as
    // production (`isDemo = false`) while routing it to the configured simulator
    // host; only bundled demo credentials are gated by `hyrotraderDemo`.
    if (simulator.rest.livenet || simulator.rest.testnet) return simulator;

    return production;
  }

  getBybitRestBaseUrl(network: 'livenet' | 'testnet') {
    const config = this.getConfiguredUrls();
    const baseUrl = config.rest[network];
    if (!baseUrl) {
      const hint =
        this.hyrotraderMode() === 'production'
          ? ' Set NEXT_PUBLIC_HYROTRADER_PROD_REST_URL, or NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL while v1 is simulator-backed.'
          : '';
      throw new Error(`HyroTrader ${config.label} REST URL is not configured.${hint}`);
    }
    return baseUrl;
  }

  getBybitPublicWsUrl(category: 'linear' | 'inverse', network: 'livenet' | 'testnet') {
    if (category !== 'linear') {
      throw new Error('HyroTrader only supports linear public WebSocket data');
    }
    const config = this.getConfiguredUrls();
    const wsUrl = config.ws.public.linear[network];
    if (!wsUrl) {
      throw new Error(`HyroTrader ${config.label} public WebSocket URL is not configured`);
    }
    return wsUrl;
  }

  getBybitPrivateWsUrl(network: 'livenet' | 'testnet') {
    const config = this.getConfiguredUrls();
    const wsUrl = config.ws.private[network];
    if (!wsUrl) {
      throw new Error(`HyroTrader ${config.label} private WebSocket URL is not configured`);
    }
    return wsUrl;
  }

  getSettleCoinsForCategory(category: string) {
    if (category === 'linear') return ['USDT'];
    this.emitAndLogError(`Unknown category ${category}`);
    throw new Error(`Unknown category ${category}`);
  }

  protected optionsEnabled() {
    return false;
  }

  protected inverseEnabled() {
    return false;
  }

  protected requiresOrderLinkId() {
    return true;
  }

  // HyroTrader rides a simulator-backed private WebSocket for both the shared
  // `/hyrotrader-demo` key and user-pasted prod challenge keys. The private WS
  // backpressure/delivery behaviour has not yet been independently verified
  // against Bybit-mainnet reliability (see the `probe-coverage` close-out in
  // `.cursor/plans/hyrotrader-audit_757b4e5d.plan.md`), so we keep the local
  // REST order-lifecycle fallback engaged in both modes. The base class gates
  // this on `isDemo`, which this subclass now derives from `hyrotraderMode`
  // for grading/history/UI purposes — but WS reliability is a transport
  // property of the simulator backend, not of the account mode, so we
  // override explicitly here. Flip to `super.shouldUseLocalRestOrderLifecycleFallback()`
  // once the probe confirms the simulator's private WS delivers placement,
  // cancel, and amend frames immediately after the REST ack.
  protected shouldUseLocalRestOrderLifecycleFallback(): boolean {
    return true;
  }
}
