import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';
import type { Execution, ExecutionsOptions, HistoricalTrade, HistoricalTradeOptions } from '../../types';

import { createHash } from 'crypto';

import rateLimit from 'axios-rate-limit';

import {
  Balance,
  ClosedPnlEntry,
  ExchangeName,
  ExchangeOptions,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderSide,
  OrderStatus,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  PublicTrade,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { loop } from '../../utils/loop';
import { subtract } from '../../utils/safe-math';
import { BaseExchange } from '../base';
import { createAPI } from './btcc.api';
import {
  BTCCAccountInfoResponse,
  BTCCAccountRaw,
  BTCCLoginResponse,
  BTCCOrderListResponse,
  BTCCOrderRaw,
  BTCCOrderStatus,
  BTCCOrderType,
  BTCCPositionListResponse,
  BTCCPositionRaw,
  BTCCPositionStatus,
  BTCCProfitReportEntry,
  BTCCProfitReportResponse,
  BTCCSymbolListResponse,
  BTCCSymbolRaw,
  BTCCTradeReportEntry,
  BTCCTradeReportResponse,
  ENDPOINTS,
} from './btcc.types';

type RequestOptions = {
  auth?: boolean;
};

const getPrecisionFromStep = (step: number | undefined): number => {
  if (!step || Number.isNaN(step)) {
    return 0;
  }
  const str = step.toString();
  if (str.includes('e-')) {
    const [, exponent] = str.split('e-');
    return parseInt(exponent, 10);
  }
  if (str.includes('.')) {
    return str.split('.')[1].length;
  }
  return 0;
};

const clampSymbol = (symbol: string) => symbol.replace(/[:\s]/g, '').toUpperCase();

export class BtccExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;

  private token: string | null = null;
  private loginPromise: Promise<void> | null = null;
  private account: BTCCAccountRaw | null = null;
  private accountNumber: string | null = null;

  private rawSymbolToDisplay = new Map<string, string>();
  private displaySymbolToRaw = new Map<string, string>();
  private symbolMetaByDisplay = new Map<string, BTCCSymbolRaw>();
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 5 });
    this.unlimitedXHR = createAPI(opts, this);
  }

  dispose = () => {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    super.dispose();
  };

  private throwUserError(message: string): never {
    this.emitAndLogError(message);
    throw new Error(message);
  }

  private ensureCredentials() {
    if (this.options.remoteSigner) {
      const message = 'BTCC does not support remote signing';
      this.emitAndLogError(message);
      throw new Error(message);
    }
    if (!this.options.key) {
      this.throwUserError('BTCC requires an API key');
    }
    if (!this.options.secret) {
      this.throwUserError('BTCC requires an API secret key');
    }
    if (!this.options.applicationId) {
      this.throwUserError('BTCC requires an account username (set Application ID field)');
    }
    if (!this.options.passphrase) {
      this.throwUserError('BTCC requires an account password (set Passphrase field)');
    }
  }

  private ensureAccountNumber(): string {
    if (this.accountNumber) {
      return this.accountNumber;
    }
    const maybeAccount = this.account?.account_no;
    if (maybeAccount) {
      this.accountNumber = String(maybeAccount);
      return this.accountNumber;
    }
    this.throwUserError('BTCC account number unavailable');
  }

  private toNumber(value: any, fallback = 0): number {
    if (value === null || value === undefined || value === '') {
      return fallback;
    }
    const num = Number(value);
    return Number.isFinite(num) ? num : fallback;
  }

  private buildSignedParams(params: Record<string, any>): Record<string, string> {
    this.ensureCredentials();
    const entries = Object.entries(params).filter(([, value]) => value !== undefined && value !== null && value !== '');

    const prepared: Record<string, string> = {};
    for (const [key, value] of entries) {
      if (typeof value === 'boolean') {
        prepared[key] = value ? '1' : '0';
      } else if (typeof value === 'number') {
        prepared[key] = Number.isFinite(value) ? value.toString() : '';
      } else {
        prepared[key] = String(value);
      }
    }

    prepared.secret_key = String(this.options.secret);

    const sortedKeys = Object.keys(prepared).sort((a, b) => a.localeCompare(b));
    const signBase = sortedKeys.map((key) => `${key}=${encodeURIComponent(prepared[key])}`).join('&');
    const sign = createHash('md5').update(signBase).digest('hex');

    return {
      ...prepared,
      sign,
    };
  }

  private async apiRequest<T>(
    endpoint: string,
    params: Record<string, any>,
    method: 'GET' | 'POST' = 'GET',
    opts: RequestOptions = {},
  ): Promise<T> {
    const requiresAuth = opts.auth !== false;

    const baseParams = { ...params };
    if (requiresAuth) {
      await this.ensureLoggedIn();
      if (this.token) {
        baseParams.token = this.token;
      }
      if (this.account?.id && baseParams.accountid === undefined) {
        baseParams.accountid = this.account.id;
      }
    }

    const signedParams = this.buildSignedParams(baseParams);

    const { data } = await this.xhr.request<any>({
      url: endpoint,
      method: method.toLowerCase(),
      params: signedParams,
    });

    if (typeof data?.code === 'number' && data.code !== 0) {
      throw new Error(data?.msg || 'BTCC API error');
    }

    return data as T;
  }

  private async ensureLoggedIn(force = false) {
    if (!force && this.token) {
      return;
    }
    if (this.loginPromise) {
      return this.loginPromise;
    }

    const loginPromise = (async () => {
      this.ensureCredentials();

      const params = {
        user_name: this.options.applicationId,
        password: this.options.passphrase,
        company_id: 1,
        api_key: this.options.key,
      };

      const data = await this.apiRequest<BTCCLoginResponse>(ENDPOINTS.LOGIN, params, 'POST', { auth: false });
      if (!data?.token) {
        throw new Error(data?.msg || 'BTCC login failed');
      }

      this.token = data.token;
      this.account = data.account;
      this.accountNumber = data.account?.account_no ? String(data.account.account_no) : null;
      this.startHeartbeat();
    })();
    this.loginPromise = loginPromise;
    try {
      await loginPromise;
    } finally {
      if (this.loginPromise === loginPromise) {
        this.loginPromise = null;
      }
    }
  }

  private startHeartbeat() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
    }

    this.heartbeatTimer = setInterval(() => {
      if (this.isDisposed || !this.token) {
        return;
      }
      this.apiRequest(ENDPOINTS.KEEPALIVE, {}, 'GET').catch((err) => {
        this.emitAndLogError(err, 'BTCC keepalive failed');
        this.token = null;
      });
    }, 20_000);
  }

  validateAccount = async () => {
    try {
      await this.ensureLoggedIn(true);
      return { error: '', balance: this.mapBalance(this.account) };
    } catch (err: any) {
      const message = err?.response?.data?.msg || err?.message || 'Invalid credentials';
      this.emitAndLogError(err, 'BTCC account validation failed');
      return { error: message };
    }
  };

  getAccount = async () => {
    await this.ensureLoggedIn();
    if (!this.account) {
      this.throwUserError('BTCC account not available after login');
    }
    const accountNo = this.ensureAccountNumber();
    return {
      userId: accountNo,
      subId: null,
      affiliateId: null,
    };
  };

  private mapRawSymbolToDisplay(rawSymbol: string) {
    const cached = this.rawSymbolToDisplay.get(rawSymbol);
    if (cached) {
      return cached;
    }
    const meta = Array.from(this.symbolMetaByDisplay.values()).find((entry) => entry.name === rawSymbol) || null;
    if (meta) {
      const display = this.buildDisplaySymbol(meta);
      this.rawSymbolToDisplay.set(rawSymbol, display);
      this.displaySymbolToRaw.set(display, rawSymbol);
      return display;
    }
    const inferred = clampSymbol(rawSymbol.split('/').pop() || rawSymbol);
    this.rawSymbolToDisplay.set(rawSymbol, inferred);
    return inferred;
  }

  private buildDisplaySymbol(meta: BTCCSymbolRaw) {
    const base = meta.base_currency || '';
    const quote = meta.profit_currency || meta.margin_currency || '';
    if (base && quote) {
      return clampSymbol(`${base}${quote}`);
    }
    const last = meta.name.split('/').pop() || meta.name;
    const match = last.match(/^([A-Z]+)([A-Z]+)(\d+)?x?$/i);
    if (match) {
      return clampSymbol(`${match[1]}${match[2]}`);
    }
    return clampSymbol(last);
  }

  private getDefaultLeverage(symbol: string) {
    const meta = this.symbolMetaByDisplay.get(symbol);
    if (!meta) {
      return 20;
    }
    const last = meta.name.split('/').pop() || '';
    const match = last.match(/(\d+)[Xx]$/);
    if (match) {
      return Number(match[1]);
    }
    if (meta.leverage_max) {
      return Number(meta.leverage_max);
    }
    return 20;
  }

  fetchMarkets = async () => {
    await this.ensureLoggedIn();
    const response = await this.apiRequest<BTCCSymbolListResponse>(ENDPOINTS.SYMBOL_LIST, {});

    const markets: Market[] = response.symbols.map((symbol) => {
      const displaySymbol = this.buildDisplaySymbol(symbol);
      this.rawSymbolToDisplay.set(symbol.name, displaySymbol);
      this.displaySymbolToRaw.set(displaySymbol, symbol.name);
      this.symbolMetaByDisplay.set(displaySymbol, symbol);

      const pricePrecision = symbol.digits ?? 2;
      const maxLeverage = this.getDefaultLeverage(displaySymbol);

      return {
        id: symbol.name,
        symbol: displaySymbol,
        base: symbol.base_currency || displaySymbol.replace(/[A-Z]+$/, ''),
        quote: symbol.profit_currency || symbol.margin_currency || 'USDT',
        active: Boolean(symbol.enable),
        precision: {
          amount: getPrecisionFromStep(symbol.volumes_step),
          price: pricePrecision,
        },
        limits: {
          amount: {
            min: Number(symbol.volumes_min ?? 0),
            max: Number(symbol.volumes_max ?? Number.MAX_SAFE_INTEGER),
          },
          leverage: {
            min: 1,
            max: maxLeverage,
          },
        },
        contractSize: symbol.contract_size ?? 1,
        isExpiring: false,
        isInverse: false,
        isSpot: false,
        info: symbol,
      };
    });

    return markets;
  };

  fetchTickers = async () => {
    // BTCC REST guide does not expose a direct ticker endpoint.
    // Return placeholder tickers derived from markets to satisfy consumers.
    const markets = this.store.markets.length ? this.store.markets : await this.fetchMarkets();
    const now = Date.now();
    const tickers: Ticker[] = markets.map((market) => ({
      id: market.id,
      symbol: market.symbol,
      bid: 0,
      ask: 0,
      last: 0,
      mark: 0,
      index: 0,
      percentage: 0,
      openInterest: 0,
      fundingRate: 0,
      fundingTime: now,
      volume: 0,
      quoteVolume: 0,
      info: {},
    }));
    return tickers;
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!this.isPublicDataOnly) {
      await this.ensureLoggedIn();
    }

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    await this.tick();
  };

  private async tick(count = 0) {
    if (this.isDisposed || this.isPublicDataOnly) return;
    try {
      const positions = await this.fetchPositions();
      if (this.isDisposed) return;
      this.store.addOrUpdatePositionsFull(positions);

      if (count % 2 === 0) {
        const balance = await this.fetchBalance();
        if (this.isDisposed) return;
        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });
      }

      const orders = await this.fetchOrders();
      if (this.isDisposed) return;
      this.store.addOrUpdateOrdersFull(orders);

      this.store.update({
        loaded: { ...this.store.loaded, positions: true, orders: true },
      });
    } catch (err) {
      this.emitAndLogError(err as Error, 'BTCC polling tick failed');
    }

    if (!this.isDisposed) {
      loop(() => this.tick(count + 1), this.getTickInterval());
    }
  }

  private mapBalance(account: BTCCAccountRaw | null | undefined): Balance {
    if (!account) {
      return {
        used: 0,
        free: 0,
        total: 0,
        upnl: 0,
      };
    }

    const total = this.toNumber(account.equity ?? account.balance, 0);
    const free = this.toNumber(account.free_margin, 0);
    const used = this.toNumber(account.margin, subtract(total, free));
    const balance = this.toNumber(account.balance, total);
    const upnl = total - balance;

    return {
      used,
      free,
      total,
      upnl,
      info: account,
    };
  }

  fetchBalance = async (): Promise<Balance> => {
    const data = await this.apiRequest<BTCCAccountInfoResponse>(ENDPOINTS.ACCOUNT_INFO, {});
    return this.mapBalance(data.account ?? this.account);
  };

  private mapPosition(raw: BTCCPositionRaw): Position {
    const symbol = this.mapRawSymbolToDisplay(raw.symbol);
    const contracts = this.toNumber(raw.volume ?? raw.exec_volume, 0);
    const entryPrice = this.toNumber(raw.open_price, 0);
    const leverage = this.toNumber(raw.multiple, this.getDefaultLeverage(symbol));
    const unrealized = this.toNumber(raw.profit ?? raw.exec_profit, 0);

    const side = raw.direction === 1 ? PositionSide.Long : PositionSide.Short;

    return {
      id: createPositionId(symbol, side, PositionMode.OneWay),
      symbol,
      side,
      entryPrice,
      notional: entryPrice * contracts,
      leverage,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
      unrealizedPnl: unrealized,
      contracts,
      liquidationPrice: this.toNumber(raw.close_price, 0),
      info: raw,
      lastUpdate: Date.now(),
    };
  }

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    const pageSize = 200;
    const params: Record<string, any> = {};

    if (symbols && symbols.length === 1) {
      params.symbol = this.getRawSymbolFromDisplay(symbols[0]);
    }

    const rawPositions: BTCCPositionRaw[] = [];
    for (let pageNo = 1; ; pageNo += 1) {
      const response = await this.apiRequest<BTCCPositionListResponse>(ENDPOINTS.POSITION_LIST, {
        ...params,
        pageNo,
        pageSize,
      });
      const page = response.positions ?? [];
      rawPositions.push(...page);
      if (page.length < pageSize || (response.total !== undefined && rawPositions.length >= response.total)) {
        break;
      }
    }

    const mapped = rawPositions
      .filter((position) => position.status === BTCCPositionStatus.Open && this.toNumber(position.volume, 0) > 0)
      .map((position) => this.mapPosition(position));

    if (symbols && symbols.length > 0) {
      const wanted = new Set(symbols.map((symbol) => symbol.toUpperCase()));
      return mapped.filter((position) => wanted.has(position.symbol.toUpperCase()));
    }

    return mapped;
  };

  private mapOrder(raw: BTCCOrderRaw): Order {
    const symbol = this.mapRawSymbolToDisplay(raw.symbol);
    const amount = this.toNumber(raw.request_volume, 0);
    const filled = this.toNumber(raw.execute_volume, 0);
    const remaining = Math.max(amount - filled, 0);

    let status = OrderStatus.Open;
    if (raw.status === BTCCOrderStatus.Filled) {
      status = OrderStatus.Closed;
    } else if (raw.status === BTCCOrderStatus.Canceled || raw.status === BTCCOrderStatus.Rejected) {
      status = OrderStatus.Canceled;
    }

    let type = OrderType.Limit;
    if (raw.type === BTCCOrderType.StopOpen) {
      type = OrderType.StopLoss;
    } else if (raw.type === BTCCOrderType.MarketStopLoss) {
      type = OrderType.StopLoss;
    } else if (raw.type === BTCCOrderType.MarketTakeProfit) {
      type = OrderType.TakeProfit;
    }

    const reduceOnly =
      raw.type === BTCCOrderType.LimitClose ||
      raw.type === BTCCOrderType.MarketStopLoss ||
      raw.type === BTCCOrderType.MarketTakeProfit;

    return {
      id: String(raw.id),
      status,
      symbol,
      type,
      side: raw.direction === 1 ? OrderSide.Buy : OrderSide.Sell,
      price: this.toNumber(raw.request_price, 0),
      amount,
      filled,
      remaining,
      reduceOnly,
      info: raw,
      lastUpdate: Date.now(),
    };
  }

  fetchOrders = async (): Promise<Order[]> => {
    const pageSize = 200;
    const rawOrders: BTCCOrderRaw[] = [];
    for (let pageNo = 1; ; pageNo += 1) {
      const response = await this.apiRequest<BTCCOrderListResponse>(ENDPOINTS.ORDER_LIST, { pageNo, pageSize });
      const page = response.orders ?? [];
      rawOrders.push(...page);
      if (page.length < pageSize || (response.total !== undefined && rawOrders.length >= response.total)) {
        break;
      }
    }

    if (!rawOrders.length) {
      return [];
    }

    return rawOrders
      .filter((order) =>
        [BTCCOrderStatus.Initial, BTCCOrderStatus.Pending, BTCCOrderStatus.PartiallyFilled].includes(order.status),
      )
      .map((order) => this.mapOrder(order));
  };

  fetchPublicTrades = async (): Promise<PublicTrade[]> => {
    return [];
  };

  fetchOHLCV = async (_opts: OHLCVOptions) => {
    return [];
  };

  private getRawSymbolFromDisplay(symbol: string) {
    const raw = this.displaySymbolToRaw.get(symbol);
    if (raw) {
      return raw;
    }
    // fallback to known entries
    const entry = Array.from(this.rawSymbolToDisplay.entries()).find(([, display]) => display === symbol);
    if (entry) {
      return entry[0];
    }
    return symbol;
  }

  private findPositionForReduceOnly(symbol: string, side: OrderSide) {
    const targetSide = side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const positions = this.store.positions.filter((position) => position.symbol === symbol);
    if (!positions.length) {
      return null;
    }
    const matching = positions.find((position) => position.side === targetSide);
    return matching ?? positions[0];
  }

  private derivePendingType(opts: PlaceOrderOpts) {
    if (opts.type === OrderType.Limit) {
      return opts.reduceOnly || opts.close ? BTCCOrderType.LimitClose : BTCCOrderType.LimitOpen;
    }
    if (opts.type === OrderType.StopLoss) {
      return opts.reduceOnly || opts.close ? BTCCOrderType.MarketStopLoss : BTCCOrderType.StopOpen;
    }
    if (opts.type === OrderType.TakeProfit) {
      return opts.reduceOnly || opts.close ? BTCCOrderType.MarketTakeProfit : BTCCOrderType.StopOpen;
    }
    return BTCCOrderType.LimitOpen;
  }

  placeOrder = async (opts: PlaceOrderOpts) => {
    await this.ensureLoggedIn();

    const rawSymbol = this.getRawSymbolFromDisplay(opts.symbol);
    const direction = opts.side === OrderSide.Buy ? 1 : 2;
    const multiple = this.getDefaultLeverage(opts.symbol);

    if (opts.type === OrderType.Market) {
      if (opts.reduceOnly || opts.close) {
        const position = this.findPositionForReduceOnly(opts.symbol, opts.side);
        if (!position?.info?.id) {
          this.throwUserError('No BTCC position available to close');
        }

        const response = await this.apiRequest(
          ENDPOINTS.CLOSE_POSITION,
          {
            symbol: rawSymbol,
            direction,
            positionid: position.info.id,
            request_volume: opts.amount,
            request_price: opts.price ?? 0,
            multiple,
          },
          'POST',
        );
        const positionId = (response as any)?.position?.id ?? position.info.id;
        return positionId ? [String(positionId)] : [];
      }

      const response = await this.apiRequest(
        ENDPOINTS.OPEN_POSITION,
        {
          symbol: rawSymbol,
          direction,
          request_volume: opts.amount,
          request_price: opts.price ?? 0,
          multiple,
        },
        'POST',
      );
      const positionId = (response as any)?.position?.id;
      return positionId ? [String(positionId)] : [];
    }

    const type = this.derivePendingType(opts);
    const payload: Record<string, any> = {
      symbol: rawSymbol,
      direction,
      type,
      request_volume: opts.amount,
      request_price: opts.price ?? opts.triggerPrice ?? 0,
      stop_loss: opts.stopLoss ?? 0,
      take_profit: opts.takeProfit ?? 0,
      multiple,
    };

    if (opts.reduceOnly || opts.close || type !== BTCCOrderType.LimitOpen) {
      const position = this.findPositionForReduceOnly(opts.symbol, opts.side);
      if (position && position.info?.id) {
        payload.positionid = position.info.id;
      }
    }

    const response = await this.apiRequest(ENDPOINTS.OPEN_PENDING, payload, 'POST');
    const orderId = (response as any)?.order?.id;
    return orderId ? [String(orderId)] : [];
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    const ids: string[] = [];
    for (const order of orders) {
      const result = await this.placeOrder(order);
      ids.push(...result);
    }
    return ids;
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const rawSymbol = this.getRawSymbolFromDisplay(order.symbol);
    const direction = order.side === OrderSide.Buy ? 1 : 2;
    const apiType =
      order.info?.type === BTCCOrderType.StopOpen ||
      order.info?.type === BTCCOrderType.MarketStopLoss ||
      order.info?.type === BTCCOrderType.MarketTakeProfit
        ? 1
        : 0;

    const payload = {
      id: Number(order.id),
      direction,
      type: apiType,
      symbol: rawSymbol,
      request_volume: 'amount' in update ? update.amount : order.amount,
      request_price: 'price' in update ? update.price : order.price,
      stop_loss: order.info?.stop_loss ?? 0,
      take_profit: order.info?.take_profit ?? 0,
    };

    await this.apiRequest(ENDPOINTS.UPDATE_PENDING, payload, 'POST');
    return [order.id];
  };

  cancelOrders = async (orders: Order[]) => {
    for (const order of orders) {
      await this.apiRequest(ENDPOINTS.CANCEL_PENDING, { id: Number(order.id) }, 'POST');
    }
  };

  cancelAllOrders = async () => {
    const currentOrders = await this.fetchOrders();
    if (!currentOrders.length) return;
    await this.cancelOrders(currentOrders);
  };

  private async fetchTradeReports(params: {
    pageNo?: number;
    pageSize?: number;
    beginTime?: number;
    endTime?: number;
    symbol?: string;
    orderId?: string;
    orderType?: number;
    direction?: number;
    contractType?: number;
    reportType?: string;
  }): Promise<BTCCTradeReportEntry[]> {
    await this.ensureLoggedIn();
    const accountNo = this.ensureAccountNumber();
    const query = {
      accountNo,
      pageNo: params.pageNo ?? 1,
      pageSize: params.pageSize ?? 50,
      beginTime: params.beginTime,
      endTime: params.endTime,
      symbol: params.symbol,
      orderId: params.orderId,
      orderType: params.orderType,
      direction: params.direction,
      contractType: params.contractType,
      reportType: params.reportType,
    };

    const response = await this.apiRequest<BTCCTradeReportResponse>(ENDPOINTS.TRADE_REPORT, query);
    return response.list ?? [];
  }

  private async fetchProfitReports(params: {
    pageNo?: number;
    pageSize?: number;
    beginTime?: number;
    endTime?: number;
    symbol?: string;
    orderId?: string;
    orderType?: number;
    openDirection?: number;
    contractType?: number;
  }): Promise<BTCCProfitReportEntry[]> {
    await this.ensureLoggedIn();
    const accountNo = this.ensureAccountNumber();
    const query = {
      accountNo,
      pageNo: params.pageNo ?? 1,
      pageSize: params.pageSize ?? 50,
      beginTime: params.beginTime,
      endTime: params.endTime,
      symbol: params.symbol,
      orderId: params.orderId,
      orderType: params.orderType,
      openDirection: params.openDirection,
      contractType: params.contractType,
    };

    const response = await this.apiRequest<BTCCProfitReportResponse>(ENDPOINTS.PROFIT_REPORT, query);
    return response.list ?? [];
  }

  private mapTradeEntryToExecution(entry: BTCCTradeReportEntry): Execution {
    const symbol = this.mapRawSymbolToDisplay(entry.symbol);
    const side = entry.direction?.toUpperCase() === 'SELL' ? OrderSide.Sell : OrderSide.Buy;
    const price = this.toNumber(entry.execPrice ?? entry.tradePrice ?? entry.openPrice, 0);
    const qty = this.toNumber(entry.execVolume ?? entry.tradeVolume, 0);
    const timestamp = this.toNumber(entry.execTime ?? entry.tradeTime ?? entry.openTime, Date.now());

    return {
      timestamp,
      symbol,
      orderId: entry.dealId ? String(entry.dealId) : undefined,
      price,
      qty,
      side,
      commission: entry.commission !== undefined ? this.toNumber(entry.commission, 0) : undefined,
      commissionAsset: entry.currency || undefined,
    };
  }

  private mapTradeEntryToHistorical(entry: BTCCTradeReportEntry): HistoricalTrade {
    const execution = this.mapTradeEntryToExecution(entry);
    return {
      symbol: execution.symbol,
      orderId: execution.orderId ?? `${execution.symbol}-${execution.timestamp}`,
      price: execution.price,
      qty: execution.qty,
      quoteQty: execution.price * execution.qty,
      realizedPnl: entry.profit !== undefined ? this.toNumber(entry.profit, 0) : undefined,
      positionSide: 'both',
      side: execution.side,
      buyer: execution.side === OrderSide.Buy,
      maker: undefined,
      commission: execution.commission,
      commissionAsset: execution.commissionAsset,
      time: execution.timestamp,
    };
  }

  private mapProfitEntryToClosedPnl(entry: BTCCProfitReportEntry): ClosedPnlEntry {
    const account = this.ensureAccountNumber();
    const price = this.toNumber(entry.tradePrice, 0);
    const volume = this.toNumber(entry.tradeVolume, 0);
    const notional = price * volume;
    const side = entry.openDirection?.toUpperCase() === 'SELL' ? 'short' : 'long';
    const displaySymbol = this.mapRawSymbolToDisplay(entry.symbol);

    return {
      symbol: displaySymbol,
      notional,
      entryPrice: this.toNumber(entry.openPrice, 0),
      exitPrice: price,
      unrealizedPnl: entry.profit !== undefined ? this.toNumber(entry.profit, 0) : 0,
      side,
      fees: entry.commission !== undefined ? this.toNumber(entry.commission, 0) : 0,
      time: this.toNumber(entry.openTime, 0),
      closedTime: this.toNumber(entry.tradeTime, 0),
      account,
      contracts: volume,
      leverage: this.toNumber(entry.multiple, this.getDefaultLeverage(displaySymbol)),
    };
  }

  fetchExecutions = async (opts: ExecutionsOptions = {}): Promise<Execution[]> => {
    const pageSize = Math.min(Math.max(opts.limit ?? 50, 1), 200);
    const symbolRaw = opts.symbol ? this.getRawSymbolFromDisplay(opts.symbol) : undefined;

    const entries = await this.fetchTradeReports({
      pageNo: 1,
      pageSize,
      beginTime: opts.from,
      endTime: opts.to,
      symbol: symbolRaw,
      orderId: opts.orderId,
    });

    return entries.map((entry) => this.mapTradeEntryToExecution(entry));
  };

  fetchHistoricalTrades = async (opts?: HistoricalTradeOptions): Promise<HistoricalTrade[]> => {
    const limit = opts?.limit;
    const pageSize = Math.min(Math.max(limit ?? 50, 1), 200);
    const symbolRaw = opts?.symbol ? this.getRawSymbolFromDisplay(opts.symbol) : undefined;

    const entries = await this.fetchTradeReports({
      pageNo: 1,
      pageSize,
      beginTime: opts?.from,
      endTime: opts?.to,
      symbol: symbolRaw,
      orderId: opts?.orderId,
    });

    return entries.map((entry) => this.mapTradeEntryToHistorical(entry));
  };

  fetchClosedPnl = async (opts?: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    const limit = opts?.limit;
    const pageSize = Math.min(Math.max(limit ?? 50, 1), 200);
    const symbolRaw = opts?.symbol ? this.getRawSymbolFromDisplay(opts.symbol) : undefined;

    const entries = await this.fetchProfitReports({
      pageNo: 1,
      pageSize,
      beginTime: opts?.from,
      endTime: opts?.to,
      symbol: symbolRaw,
      orderId: opts?.orderId,
    });

    return entries.map((entry) => this.mapProfitEntryToClosedPnl(entry));
  };

  setLeverage = async () => {
    this.throwUserError('BTCC API does not expose leverage adjustment endpoint');
  };

  changePositionMode = async () => {
    this.throwUserError('BTCC API does not support position mode changes via API');
  };

  changeMarginType = async () => {
    this.throwUserError('BTCC API does not expose margin type changes');
  };

  fetchSymbolMaxLeverage = async (symbol: string) => {
    return this.getDefaultLeverage(symbol);
  };

  listenOrderBook = (_symbol: string, _callback: (orderBook: any) => void) => {
    return () => {};
  };

  listenOHLCV = (_opts: OHLCVOptions, _callback: (candle: any) => void) => {
    return () => {};
  };

  listenOHLCVs = (_opts: { symbols: string[]; interval: Timeframe }, _callback: (candle: any) => void) => {};

  listenOHLCVWebworker = (_opts: Pick<OHLCVOptions, 'symbol' | 'interval'>, _callback: (candle: any) => void) => {
    return () => {};
  };

  listenOrderBookWebworker = (_symbol: string, _callback: (orderBook: any) => void) => {
    return () => {};
  };
}
