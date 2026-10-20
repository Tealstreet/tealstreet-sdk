import type { AxiosInstance } from 'axios';
import type { Store } from '../../../store/store.interface';
import type { Candle, ExchangeName, ExchangeOptions, ExecutionsOptions, IncomeHistoryOptions, OHLCVOptions, Order, OrderBook, OrderHistoryOptions, PlaceOrderOpts, UpdateOrderOpts } from '../../../types';

import { ethers } from 'ethers';
import { BaseExchange } from '../../../exchanges/base';
import { createAPI, createPublicAPI } from './txflow.api';
import { TxflowAccount } from './txflow.account';
import { TxFlowTrading, txflowClientOrderIdRule } from './txflow.trading';
import { fetchHistoricalFills, fetchHistoricalOrders, fetchIncomeHistory, fetchIncomeHistoryRange, fetchTrades } from './txflow.history';
import { emptyTxFlowTicker, mapTxFlowCandle, mapTxFlowHistoricalIdentities, mapTxFlowMarket, mapTxFlowTicker } from './txflow.public';
import { TXFLOW_INTERVALS, TXFLOW_PUBLIC_READS, TxFlowCandle, TxFlowHistoricalIdentity, TxFlowInstrument, TxFlowTicker } from './txflow.types';
import { TxFlowWebSocket } from './txflow.ws-public';

export class TxFlowExchange extends BaseExchange {
  xhr: AxiosInstance;
  publicXhr: AxiosInstance;
  account: TxflowAccount;
  trading: TxFlowTrading;
  historicalIdentities = new Map<string, TxFlowHistoricalIdentity>();
  nodeSocket: TxFlowWebSocket;
  apiSocket: TxFlowWebSocket;
  private accountTimer?: ReturnType<typeof setTimeout>;
  protected readonly clientOrderIdRule = txflowClientOrderIdRule;
  get isPublicDataOnly(): boolean { return this.options.publicDataOnly === true; }
  protected override get canSign(): boolean { return Boolean(this.options.secret && this.options.applicationId); }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);
    this.xhr = createAPI(opts, this);
    this.publicXhr = createPublicAPI(opts, this);
    this.account = new TxflowAccount({
      store: this.store,
      emitFill: (fill) => this.emitter.emit('fill', fill),
      reportError: (error) => this.emitAndLogError(error),
      requestInfo: (body) => this.requestInfo(body),
      wallet: () => this.wallet(),
      marketById: (id) => this.marketById(String(id)),
      disposed: () => this.isDisposed,
    });
    this.trading = new TxFlowTrading(this);
    this.nodeSocket = new TxFlowWebSocket(this, false);
    this.apiSocket = new TxFlowWebSocket(this, true);
    if (opts.publicDataOnly) this.publicWebsocket = this.nodeSocket;
    else this.privateWebsocket = this.nodeSocket;
    this.publicWebsocketSecondary = this.apiSocket;
  }

  reject(message: string): never { return this.rejectPreflight(message); }
  wallet = (): string => this.options.vault || this.options.applicationId || '';
  marketById = (id: string) => this.store.markets.find((market) => market.id === id || market.symbol.toLowerCase() === id.toLowerCase() || market.base.toLowerCase() === id.toLowerCase());

  info = async <T>(body: Record<string, unknown>): Promise<T> => {
    if (!this.options.testnet) return this.reject('TxFlow mainnet is offline; testnet is required');
    const xhr = TXFLOW_PUBLIC_READS.has(String(body.type)) ? this.publicXhr : this.xhr;
    const response = await xhr.post<{ success: boolean; code: number; data: T; message?: string }>('/info', body);
    if (response.status < 200 || response.status >= 300 || response.data?.success !== true || response.data.code !== 0 || response.data.data == null) {
      throw new Error(response.data?.message || `Invalid TxFlow info response (HTTP ${response.status})`);
    }
    return response.data.data;
  };
  requestInfo = <T>(body: Record<string, unknown>): Promise<T> => this.info<T>(body);

  start = async (): Promise<void> => {
    if (!this.options.testnet) this.reject('TxFlow mainnet is offline; testnet is required');
    try { if (this.options.vault) ethers.getAddress(this.options.vault); } catch { this.reject('Invalid TxFlow vault address'); }
    if (!this.options.publicDataOnly) {
      try {
        ethers.getAddress(this.options.applicationId ?? '');
        new ethers.Wallet(this.options.secret ?? '');
        if (this.options.vault) ethers.getAddress(this.options.vault);
      } catch { this.reject('Invalid TxFlow main-wallet address or agent secret'); }
    }
    const external = await this.initExternalPublicData();
    if (!external) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;
      this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });
      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;
      this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
      this.apiSocket.connectAndSubscribe();
      this.nodeSocket.connectAndSubscribe();
    }
    if (this.options.publicDataOnly) {
      this.store.update({ orders: [], positions: [], balance: { free: 0, used: 0, total: 0, upnl: 0 }, loaded: { ...this.store.loaded, orders: true, positions: true, balance: true } });
    } else {
      await this.account.start();
      this.scheduleAccount();
      if (external && !this.isDisposed) this.nodeSocket.connectAndSubscribe();
    }
  };

  fetchMarkets = async () => {
    const response = await this.info<{ universe: TxFlowInstrument[] }>({ type: 'perpMeta', includeDelisted: true });
    if (!Array.isArray(response.universe)) throw new Error('Invalid TxFlow metadata');
    const identities = mapTxFlowHistoricalIdentities(response.universe);
    const markets = response.universe.flatMap((row) => { const market = mapTxFlowMarket(row); return market ? [market] : []; });
    if (!markets.length || new Set(markets.map((m) => m.id)).size !== markets.length || new Set(markets.map((m) => m.symbol)).size !== markets.length) throw new Error('Invalid or duplicate TxFlow market identities');
    this.historicalIdentities = identities;
    return markets;
  };
  fetchTickers = async () => {
    const tickers = [];
    for (const market of this.store.markets) {
      if (this.isDisposed) return tickers;
      const row = await this.info<TxFlowTicker>({ type: 'activeAssetCtx', coin: market.id });
      tickers.push(mapTxFlowTicker(emptyTxFlowTicker(market), row, true));
      await new Promise<void>((resolve) => setTimeout(resolve, 25));
    }
    return tickers;
  };
  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) return this.reject('Market not found');
    if (!TXFLOW_INTERVALS.has(opts.interval)) return this.reject('Unsupported candle interval');
    const endTime = opts.to == null ? Date.now() : opts.to * 1000;
    const interval = Number(opts.interval.slice(0, -1)) * ({ m: 60_000, h: 3_600_000, d: 86_400_000 }[opts.interval.slice(-1)] ?? 0);
    const startTime = opts.from == null ? Math.max(0, endTime - interval * Math.min(opts.limit ?? 500, 1500)) : opts.from * 1000;
    if (!Number.isFinite(startTime) || !Number.isFinite(endTime) || startTime < 0 || startTime > endTime || endTime > Number.MAX_SAFE_INTEGER) return this.reject('Invalid candle time range');
    const rows = await this.info<TxFlowCandle[]>({ type: 'candleSnapshot', req: { coin: market.id, interval: opts.interval, startTime: Math.floor(startTime), endTime: Math.floor(endTime) } });
    if (!Array.isArray(rows)) throw new Error('Invalid TxFlow candle response');
    return rows.map(mapTxFlowCandle);
  };
  listenOrderBook = (symbol: string, callback: (book: OrderBook) => void) => this.externalPublicDataProvider ? this.externalPublicDataProvider.subscribeOrderbook(symbol, callback) : this.nodeSocket.listenOrderBook(symbol, callback);
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => this.externalPublicDataProvider ? this.externalPublicDataProvider.subscribeCandle(opts, callback) : this.apiSocket.listenOHLCV(opts, callback);

  fetchOrders = () => this.account.fetchOrders();
  fetchPositions = () => this.account.fetchPositions();
  fetchBalance = () => this.account.fetchBalance();
  placeOrder = async (opts: PlaceOrderOpts) => { this.resolveCallerClientOrderId(opts); return this.trading.placeOrders([opts]); };
  placeOrders = async (opts: PlaceOrderOpts[]) => { this.assertCallerClientOrderIds(opts); return this.trading.placeOrders(opts); };
  cancelOrders = (orders: Order[]) => this.trading.cancelOrders(orders);
  cancelSymbolOrders = (symbol: string) => this.trading.cancelSymbolOrders(symbol);
  updateOrder = (opts: UpdateOrderOpts) => this.trading.updateOrder(opts);
  setLeverage = (symbol: string, leverage: number) => this.trading.setLeverage(symbol, leverage);
  changeMarginType = (isolated: boolean, symbol?: string) => this.trading.changeMarginType(isolated, symbol);
  changePositionMode = (hedged: boolean) => this.trading.changePositionMode(hedged);
  fetchHistoricalFills = (opts: ExecutionsOptions) => fetchHistoricalFills(this, opts);
  fetchHistoricalOrders = (opts: OrderHistoryOptions = {}) => fetchHistoricalOrders(this, opts);
  fetchIncomeHistory = (opts: IncomeHistoryOptions = {}) => fetchIncomeHistory(this, opts);
  fetchIncomeHistoryRange = (start: number, end: number, type?: string) => fetchIncomeHistoryRange(this, start, end, type);
  fetchTrades = fetchTrades;
  private scheduleAccount() {
    if (this.isDisposed) return;
    this.accountTimer = setTimeout(() => {
      this.accountTimer = undefined;
      void this.account.refresh().catch((error) => this.emitAndLogError(String(error))).finally(() => this.scheduleAccount());
    }, 5000);
  }
  dispose() { if (this.accountTimer) clearTimeout(this.accountTimer); this.accountTimer = undefined; this.account.dispose(); super.dispose(); }
}
