import type { Candle, OHLCVOptions, OrderBook } from '../../../types';
import type { TxFlowExchange } from './txflow.exchange';
import type { TxFlowCandle, TxFlowContext, TxFlowTicker } from './txflow.types';

import { calcOrderBookTotal, sortOrderBook } from '../../../utils/orderbook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { mapTxFlowCandle, mapTxFlowContext, mapTxFlowTicker, txflowNumber } from './txflow.public';
import { TXFLOW_TESTNET_REST_URL } from './txflow.types';

export class TxFlowWebSocket extends BaseWebSocket<TxFlowExchange> {
  private heartbeat?: ReturnType<typeof setTimeout>;
  private subscriptionTimer?: ReturnType<typeof setTimeout>;
  private readonly subscriptionQueue: string[] = [];
  private subscriptionsSuspended = false;
  private readonly bookListeners = new Map<string, Set<(book: OrderBook) => void>>();
  private readonly candleListeners = new Map<string, Set<(candle: Candle) => void>>();
  private readonly bookTimes = new Map<string, number>();

  constructor(parent: TxFlowExchange, readonly api: boolean) { super(parent); }

  connectAndSubscribe = () => {
    if (this.isDisposed) return;
    const url = `${TXFLOW_TESTNET_REST_URL.replace('https:', 'wss:')}/${this.api ? 'api-ws' : 'ws'}`;
    const socket = this.createWebsocket(url);
    this.ws = socket;
    socket.addEventListener('open', () => { if (this.ws === socket) this.opened(); });
    socket.addEventListener('message', (event: MessageEvent) => { if (this.ws === socket) this.onMessage(event); });
    socket.addEventListener('close', (event: unknown) => {
      if (this.ws !== socket) return;
      this.stopHeartbeat();
      this.stopSubscriptions();
      if (!this.api) {
        this.bookTimes.clear();
        for (const listeners of this.bookListeners.values()) for (const listener of listeners) listener({ bids: [], asks: [] });
        this.parent.account.disconnected();
      }
      this.onClose(event);
    });
  };

  private opened = () => {
    if (this.isDisposed) return;
    this.stopSubscriptions();
    this.subscriptionsSuspended = false;
    this.trackLastReceived();
    if (this.api) {
      for (const market of this.store.markets) this.subscription('subscribe', { type: 'ticker', coin: market.base, instrumentId: Number(market.id) });
      for (const key of this.candleListeners.keys()) {
        const [symbol, interval] = key.split('|');
        const market = this.store.marketsBySymbol[symbol];
        if (market) this.subscription('subscribe', { type: 'candle', coin: market.id, interval });
      }
    } else {
      this.subscription('subscribe', { type: 'allAssetCtxs' });
      for (const symbol of this.bookListeners.keys()) {
        const market = this.store.marketsBySymbol[symbol];
        if (market) this.subscription('subscribe', { type: 'l2Book', coin: market.id });
      }
      if (!this.parent.options.publicDataOnly) {
        for (const type of ['orderUpdates', 'clearinghouseState', 'userAssetData']) this.subscription('subscribe', { type, user: this.parent.wallet() });
        void this.parent.account.refresh().catch((error) => this.parent.emitAndLogError(error));
      }
    }
    this.startHeartbeat();
  };

  private stopHeartbeat() {
    if (this.heartbeat) clearTimeout(this.heartbeat);
    this.heartbeat = undefined;
  }
  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeat = setTimeout(() => {
      if (this.isDisposed || !this.isConnected) return;
      this.ping();
      this.startHeartbeat();
    }, 10_000);
  }
  ping = () => { if (this.isConnected) this.ws?.send('{"method":"ping"}'); };

  private subscription(method: string, subscription: Record<string, unknown>) {
    if (!this.isConnected || this.subscriptionsSuspended) return;
    if (this.subscriptionQueue.length >= 4096) {
      this.stopSubscriptions();
      this.subscriptionsSuspended = true;
      this.parent.emitAndLogError('TxFlow subscription queue exhausted');
      void this.reconnect();
      return;
    }
    this.subscriptionQueue.push(JSON.stringify({ method, ...(!this.api ? { type: subscription.type } : {}), subscription }));
    if (!this.subscriptionTimer) this.flushSubscription();
  }

  private stopSubscriptions() {
    if (this.subscriptionTimer) clearTimeout(this.subscriptionTimer);
    this.subscriptionTimer = undefined;
    this.subscriptionQueue.length = 0;
  }
  private flushSubscription = () => {
    this.subscriptionTimer = undefined;
    if (this.isDisposed || !this.isConnected) { this.stopSubscriptions(); return; }
    const frame = this.subscriptionQueue.shift();
    if (frame === undefined) return;
    this.ws?.send(frame);
    // The gateway drops subscription bursts without identifying the lost frame.
    this.subscriptionTimer = setTimeout(this.flushSubscription, 150);
  }

  onMessage = (event: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();
    const text = String(event.data);
    try {
      const frame = JSON.parse(text);
      const data = frame.data;
      if ((frame.channel === 'error' || frame.type === 'error') && (frame.code ?? data?.code) === 'RISK_RATE_LIMITED') {
        if (this.subscriptionsSuspended) return;
        this.stopSubscriptions();
        this.subscriptionsSuspended = true;
        void this.reconnect();
        return;
      }
      if (frame.method === 'PONG' || frame.channel === 'pong') return;
      if (frame.channel === 'allAssetCtxs' || frame.channel === 'activeAssetCtx') {
        const rows = frame.channel === 'allAssetCtxs' ? data.ctxs : [data];
        for (const row of rows as Array<{ coin: string; ctx: TxFlowContext }>) {
          const market = this.parent.marketById(row.coin);
          const ticker = market && this.store.tickersBySymbol[market.symbol];
          if (ticker) this.store.updateTicker(ticker, mapTxFlowContext(ticker, row.ctx));
        }
      } else if (frame.channel === 'ticker') {
        const row = data as TxFlowTicker;
        const market = this.parent.marketById(String(row.instrumentId));
        const ticker = market && this.store.tickersBySymbol[market.symbol];
        if (ticker) this.store.updateTicker(ticker, mapTxFlowTicker(ticker, row));
      } else if (frame.channel === 'l2Book') {
        const market = this.parent.marketById(String(data.coin));
        if (!market || !this.bookListeners.has(market.symbol) || data.time < (this.bookTimes.get(market.symbol) ?? 0)) return;
        const levels = data.levels as Array<Array<{ px: string; sz: string }>>;
        if (levels.length !== 2 || levels.flat().some((level) => txflowNumber(level.px) <= 0 || txflowNumber(level.sz) < 0)) return;
        const map = (rows: Array<{ px: string; sz: string }>) => rows.filter((row) => txflowNumber(row.sz) > 0).map((row) => ({ price: txflowNumber(row.px), amount: txflowNumber(row.sz), total: 0 }));
        const book = { bids: map(levels[0]), asks: map(levels[1]) };
        sortOrderBook(book);
        calcOrderBookTotal(book);
        this.bookTimes.set(market.symbol, data.time);
        for (const listener of this.bookListeners.get(market.symbol) ?? []) listener(book);
      } else if (frame.channel === 'candle') {
        for (const row of (Array.isArray(data) ? data : [data]) as TxFlowCandle[]) {
          const market = this.parent.marketById(row.s);
          if (market) for (const listener of this.candleListeners.get(`${market.symbol}|${row.i}`) ?? []) listener(mapTxFlowCandle(row));
        }
      }
      if (this.api) this.parent.account.apiFrame(text);
      else this.parent.account.nodeFrame(text);
    } catch (error) {
      this.parent.emitAndLogError(String(error));
    }
  };

  listenOrderBook = (symbol: string, callback: (book: OrderBook) => void) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) return () => {};
    let listeners = this.bookListeners.get(symbol);
    if (!listeners) {
      listeners = new Set();
      this.bookListeners.set(symbol, listeners);
      this.subscription('subscribe', { type: 'l2Book', coin: market.id });
    }
    listeners.add(callback);
    return () => {
      listeners?.delete(callback);
      if (listeners?.size === 0) {
        this.bookListeners.delete(symbol);
        this.bookTimes.delete(symbol);
        this.subscription('unsubscribe', { type: 'l2Book', coin: market.id });
      }
    };
  };
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) return () => {};
    const key = `${opts.symbol}|${opts.interval}`;
    let listeners = this.candleListeners.get(key);
    if (!listeners) {
      listeners = new Set();
      this.candleListeners.set(key, listeners);
      this.subscription('subscribe', { type: 'candle', coin: market.id, interval: opts.interval });
    }
    listeners.add(callback);
    return () => {
      listeners?.delete(callback);
      if (listeners?.size === 0) {
        this.candleListeners.delete(key);
        this.subscription('unsubscribe', { type: 'candle', coin: market.id, interval: opts.interval });
      }
    };
  };
  dispose() { this.stopHeartbeat(); this.stopSubscriptions(); this.bookListeners.clear(); this.candleListeners.clear(); super.dispose(); }
}
