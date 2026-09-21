import type { Candle, OHLCVOptions, OrderBook } from '../../types';
import type { MexcExchange } from './mexc.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity } from '../../types';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, INTERVAL, isMexcControlChannel, MEXC_WS_HEARTBEAT_INTERVAL_MS } from './mexc.types';
import { fromMexcSymbol, normalizeCandle, normalizeOrderBook, normalizeTicker, toMexcSymbol } from './mexc.utils';

type MessageHandler = (json: Record<string, any>) => void;
type Subscription = {
  method: string;
  param?: Record<string, any>;
};

const subscriptionKey = (subscription: Subscription): string => JSON.stringify(subscription);

export class MexcPublicWebsocket extends BaseWebSocket<MexcExchange> {
  protected override treatAnyMessageAsPong = true;

  messageHandlers: Record<string, MessageHandler[]> = {};
  private subscriptions = new Map<string, Subscription>();
  private tickerSubscriptions = new Map<string, Subscription>();
  private orderbooks: Record<string, OrderBook> = {};
  private heartbeatInterval?: NodeJS.Timeout;

  constructor(parent: MexcExchange) {
    super(parent);
    this.parent.on('activeTickerSymbols:changed', this.handleActiveTickerSymbolsChanged);
  }

  connectAndSubscribe = () => {
    if (this.isDisposed) return;

    const key = this.parent.options.testnet ? 'testnet' : 'livenet';
    this.ws = this.createWebsocket(BASE_WS_URL.public[key], undefined, { headers: { 'User-Agent': 'Tealstreet' } });
    this.ws.addEventListener('open', this.onOpen);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.handleClose);
  };

  onOpen = () => {
    if (this.isDisposed) return;
    this.startHeartbeat();
    this.reconcileTickerSubscriptions();
    this.resubscribe();
    this.emitWsReconnected();
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithDiagnostics(JSON.stringify({ method: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();

    const json = bigIntAsStringJsonParse<Record<string, any>>(String(data));
    if (!json) return;
    this.emitReceiveDiagnostics(data, json);

    if (json.channel === 'pong' || json.msg === 'pong' || json.method === 'pong') {
      this.handlePongEvent(15_000);
      return;
    }

    if (json.success === false) {
      this.parent.log(`MEXC public WS subscription failed: ${json.message || json.msg || 'unknown error'}`, LogSeverity.Warning);
      return;
    }

    const channel = String(json.channel || '');
    if (!channel) return;
    if (!isMexcControlChannel(channel)) {
      this.recordMessageHandlerStats(channel);
    }

    switch (channel) {
      case 'push.ticker':
        this.handleTicker(json);
        break;
      case 'push.depth':
      case 'push.depth.full':
        this.handleDepth(json);
        break;
      case 'push.kline':
        this.handleKline(json);
        break;
      case 'push.deal':
        this.handleRoutedMessage(`trade.${fromMexcSymbol(json.symbol)}`, json);
        break;
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const interval = INTERVAL[opts.interval];
    if (!interval) {
      const message = `MEXC public WebSocket does not support ${opts.interval} candles`;
      this.parent.log(message, LogSeverity.Warning);
      throw new Error(message);
    }

    const handlerKey = `kline.${opts.symbol}.${interval}`;
    const subscription = {
      method: 'sub.kline',
      param: {
        symbol: toMexcSymbol(opts.symbol),
        interval,
      },
    };
    const handler = (json: Record<string, any>) => {
      const candle = normalizeCandle(json.data || json);
      if (candle) callback(candle);
    };

    this.addHandler(handlerKey, handler);
    this.subscribe(subscription);

    return () => {
      this.removeHandler(handlerKey, handler);
      this.unsubscribe(subscription);
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    const handlerKey = `depth.${symbol}`;
    const subscription = {
      method: 'sub.depth.full',
      param: {
        symbol: market.id,
        limit: 20,
      },
    };

    const handler = () => {
      const orderbook = this.orderbooks[symbol];
      if (orderbook) callback(orderbook);
    };

    this.addHandler(handlerKey, handler);
    this.parent
      .fetchOrderBook(symbol, 100)
      .then((snapshot) => {
        if (this.isDisposed) return;
        this.orderbooks[symbol] = snapshot;
        callback(snapshot);
        this.subscribe(subscription);
      })
      .catch((err: any) => {
        this.parent.log(`MEXC orderbook snapshot failed for ${symbol}: ${err?.message || err}`, LogSeverity.Warning);
      });

    return () => {
      this.removeHandler(handlerKey, handler);
      this.unsubscribe(subscription);
      delete this.orderbooks[symbol];
    };
  };

  override dispose() {
    this.stopHeartbeat();
    this.parent.off('activeTickerSymbols:changed', this.handleActiveTickerSymbolsChanged);
    super.dispose();
  }

  private startHeartbeat = () => {
    this.stopHeartbeat();
    this.ping();
    this.heartbeatInterval = setInterval(() => {
      if (!this.isDisposed && this.isConnected) {
        this.ping();
      }
    }, MEXC_WS_HEARTBEAT_INTERVAL_MS);
  };

  private stopHeartbeat = () => {
    clearInterval(this.heartbeatInterval);
    this.heartbeatInterval = undefined;
  };

  private handleClose = (event?: any) => {
    this.stopHeartbeat();
    this.onClose(event);
  };

  private handleActiveTickerSymbolsChanged = () => {
    if (this.isDisposed) return;
    this.reconcileTickerSubscriptions();
  };

  private reconcileTickerSubscriptions = () => {
    const desired = new Map<string, Subscription>();
    const availableSymbols = this.store.markets.map((market) => market.symbol);
    const activeSymbols = this.parent.getActiveTickerSymbols();
    const activeSet = new Set(activeSymbols);
    const symbols = activeSymbols.length ? availableSymbols.filter((symbol) => activeSet.has(symbol)) : availableSymbols;

    for (const symbol of symbols.slice(0, 100)) {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) continue;
      const subscription = { method: 'sub.ticker', param: { symbol: market.id } };
      desired.set(subscriptionKey(subscription), subscription);
    }

    for (const [key, subscription] of this.tickerSubscriptions) {
      if (!desired.has(key)) {
        this.unsubscribe(subscription, false);
        this.tickerSubscriptions.delete(key);
      }
    }

    for (const [key, subscription] of desired) {
      if (!this.tickerSubscriptions.has(key)) {
        this.tickerSubscriptions.set(key, subscription);
        this.subscribe(subscription, false);
      }
    }
  };

  private subscribe = (subscription: Subscription, track = true) => {
    const key = subscriptionKey(subscription);
    if (track) this.subscriptions.set(key, subscription);
    if (!this.isConnected) return;
    this.sendWithDiagnostics(JSON.stringify(subscription));
  };

  private unsubscribe = (subscription: Subscription, track = true) => {
    const key = subscriptionKey(subscription);
    if (track) this.subscriptions.delete(key);
    if (!this.isConnected) return;
    this.sendWithDiagnostics(JSON.stringify({ ...subscription, method: subscription.method.replace(/^sub\./, 'unsub.') }));
  };

  private resubscribe = () => {
    for (const subscription of this.subscriptions.values()) {
      this.sendWithDiagnostics(JSON.stringify(subscription));
    }
    for (const subscription of this.tickerSubscriptions.values()) {
      this.sendWithDiagnostics(JSON.stringify(subscription));
    }
  };

  private sendWithDiagnostics = (data: string) => {
    this.send(data);
    this.emitWsDiagnosticSend(data, {
      format: (message) => `[mexc public] ${message}`,
    });
  };

  private emitReceiveDiagnostics = (data: unknown, json: Record<string, any>) => {
    this.emitWsDiagnosticReceive(data, {
      allowChannel: () => json.channel !== 'push.ticker' && json.channel !== 'push.depth' && json.channel !== 'push.depth.full',
      format: (message) => `[mexc public] ${String(message)}`,
    });
  };

  private addHandler = (key: string, handler: MessageHandler) => {
    this.messageHandlers[key] = [...(this.messageHandlers[key] || []), handler];
  };

  private removeHandler = (key: string, handler: MessageHandler) => {
    this.messageHandlers[key] = (this.messageHandlers[key] || []).filter((item) => item !== handler);
    if (!this.messageHandlers[key].length) this.cleanupMessageHandlerStats(key);
  };

  private handleRoutedMessage = (key: string, json: Record<string, any>) => {
    const handlers = this.messageHandlers[key];
    if (!handlers?.length) return;
    for (const handler of handlers) handler(json);
  };

  private handleTicker = (json: Record<string, any>) => {
    const symbol = fromMexcSymbol(json.symbol ?? json.data?.symbol);
    const market = this.store.marketsBySymbol[symbol];
    const current = this.store.tickersBySymbol[symbol];
    if (!market || !current) return;

    const next = normalizeTicker({ ...json.data, symbol: market.id }, market);
    this.store.updateTicker(current, {
      bid: next.bid,
      ask: next.ask,
      last: next.last,
      mark: next.mark,
      index: next.index,
      percentage: next.percentage,
      openInterest: next.openInterest,
      fundingRate: next.fundingRate,
      fundingTime: next.fundingTime,
      volume: next.volume,
      quoteVolume: next.quoteVolume,
    });
  };

  private handleDepth = (json: Record<string, any>) => {
    const symbol = fromMexcSymbol(json.symbol ?? json.data?.symbol);
    const market = this.store.marketsBySymbol[symbol];
    if (!market) return;
    this.orderbooks[symbol] = normalizeOrderBook(json.data || json, market);
    this.handleRoutedMessage(`depth.${symbol}`, json);
  };

  private handleKline = (json: Record<string, any>) => {
    const symbol = fromMexcSymbol(json.symbol ?? json.data?.symbol);
    const interval = String(json.data?.interval || json.data?.i || '');
    this.handleRoutedMessage(`kline.${symbol}.${interval}`, json);
  };
}
