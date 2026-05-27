import type { Candle, OHLCVOptions, OrderBook } from '../../types';
import type { WeexExchange } from './weex.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity } from '../../types';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { updateOrderBookByPrice } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, INTERVAL } from './weex.types';
import { normalizeCandle, toIntegerString, toNumber } from './weex.utils';

type MessageHandler = (json: Record<string, any>) => void;

export class WeexPublicWebsocket extends BaseWebSocket<WeexExchange> {
  protected override treatAnyMessageAsPong = true;

  messageHandlers: Record<string, MessageHandler[]> = {};
  private topics = new Set<string>();
  private subscriptionId = 1;
  private orderbooks: Record<string, OrderBook & { lastUpdateId?: string }> = {};

  connectAndSubscribe = () => {
    if (this.isDisposed) return;

    const key = this.parent.options.testnet ? 'testnet' : 'livenet';
    this.ws = this.createWebsocket(BASE_WS_URL.public[key], undefined, { headers: { 'User-Agent': 'Tealstreet' } });
    this.ws.addEventListener('open', this.onOpen);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.onClose);
  };

  onOpen = () => {
    if (this.isDisposed) return;

    this.subscribeTickerTopics();
    this.resubscribeTopics();
    this.emitWsReconnected();
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ method: 'PING', id: this.subscriptionId++ }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();

    const json = bigIntAsStringJsonParse<Record<string, any>>(String(data));
    if (!json) return;

    if (json.event === 'ping') {
      this.send(JSON.stringify({ method: 'PONG', id: this.subscriptionId++ }));
      this.handlePongEvent(15_000);
      return;
    }

    if (json.result !== undefined) {
      return;
    }

    const event = json.e;
    const symbol = json.s;
    if (!event) return;

    const topic = symbol ? `${symbol}@${event}` : event;
    this.recordMessageHandlerStats(topic);

    switch (event) {
      case 'ticker':
        this.handleTicker(json);
        break;
      case 'depth':
        this.handleDepth(json);
        break;
      case 'kline':
        this.handleRoutedMessage(`kline.${symbol}.${json.d?.[0]?.i || ''}`, json);
        break;
      case 'trade':
        this.handleRoutedMessage(`trade.${symbol}`, json);
        break;
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const interval = INTERVAL[opts.interval];
    const topic = `${opts.symbol}@kline_${interval}_LAST_PRICE`;
    const handlerKey = `kline.${opts.symbol}.${interval}`;

    const handler = (json: Record<string, any>) => {
      const rows = Array.isArray(json.d) ? json.d : [];
      for (const row of rows) {
        const candle = normalizeCandle(row);
        if (candle) callback(candle);
      }
    };

    this.addHandler(handlerKey, handler);
    this.subscribeTopic(topic);

    return () => {
      this.removeHandler(handlerKey, handler);
      this.unsubscribeTopic(topic);
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const handlerKey = `depth.${symbol}`;
    const topic = `${symbol}@depth200`;

    const handler = () => {
      const orderbook = this.orderbooks[symbol];
      if (orderbook) {
        callback(orderbook);
      }
    };

    this.addHandler(handlerKey, handler);

    this.parent
      .fetchOrderBook(symbol, 200)
      .then((orderbook) => {
        if (this.isDisposed) return;
        this.orderbooks[symbol] = orderbook;
        callback(orderbook);
        this.subscribeTopic(topic);
      })
      .catch((err: any) => {
        this.parent.log(`WEEX orderbook snapshot failed for ${symbol}: ${err?.message || err}`, LogSeverity.Warning);
      });

    return () => {
      this.removeHandler(handlerKey, handler);
      this.unsubscribeTopic(topic);
      delete this.orderbooks[symbol];
    };
  };

  private subscribeTickerTopics = () => {
    const availableSymbols = this.store.markets.map((market) => market.symbol);
    const activeSymbols = this.parent.getActiveTickerSymbols();
    const activeSet = new Set(activeSymbols);
    const symbols = (
      activeSymbols.length ? availableSymbols.filter((symbol) => activeSet.has(symbol)) : availableSymbols
    ).slice(0, 90);
    this.subscribeTopics(
      symbols.map((symbol) => `${symbol}@ticker`),
      false,
    );
  };

  private resubscribeTopics = () => {
    const topics = Array.from(this.topics);
    if (topics.length) {
      this.subscribeTopics(topics, false);
    }
  };

  private subscribeTopic = (topic: string) => {
    this.topics.add(topic);
    this.subscribeTopics([topic]);
  };

  private unsubscribeTopic = (topic: string) => {
    this.topics.delete(topic);
    if (!this.isConnected) return;
    this.send(JSON.stringify({ method: 'UNSUBSCRIBE', params: [topic], id: this.subscriptionId++ }));
  };

  private subscribeTopics = (topics: string[], track = true) => {
    if (track) {
      topics.forEach((topic) => this.topics.add(topic));
    }
    if (!this.isConnected || topics.length === 0) return;

    for (let i = 0; i < topics.length; i += 50) {
      this.send(JSON.stringify({ method: 'SUBSCRIBE', params: topics.slice(i, i + 50), id: this.subscriptionId++ }));
    }
  };

  private addHandler = (key: string, handler: MessageHandler) => {
    this.messageHandlers[key] = [...(this.messageHandlers[key] || []), handler];
  };

  private removeHandler = (key: string, handler: MessageHandler) => {
    this.messageHandlers[key] = (this.messageHandlers[key] || []).filter((item) => item !== handler);
    if (!this.messageHandlers[key].length) {
      delete this.messageHandlers[key];
      this.cleanupMessageHandlerStats(key);
    }
  };

  private handleRoutedMessage = (key: string, json: Record<string, any>) => {
    const handlers = this.messageHandlers[key];
    if (!handlers?.length) return;
    for (const handler of handlers) {
      handler(json);
    }
  };

  private handleTicker = (json: Record<string, any>) => {
    const ticker = this.store.tickersBySymbol[json.s];
    const update = Array.isArray(json.d) ? json.d[0] : json.d;
    if (!ticker || !update) return;

    const last = toNumber(update.c, ticker.last);
    const mark = toNumber(update.m, ticker.mark);
    const index = toNumber(update.i, ticker.index);
    const percentage = toNumber(update.P, ticker.percentage);

    this.store.updateTicker(ticker, {
      last,
      mark,
      index,
      percentage: Math.abs(percentage) <= 1 ? percentage * 100 : percentage,
      volume: toNumber(update.v, ticker.volume),
      quoteVolume: toNumber(update.q, ticker.quoteVolume),
    });
  };

  private handleDepth = (json: Record<string, any>) => {
    const symbol = json.s;
    const orderbook = this.orderbooks[symbol];
    if (!orderbook) return;

    const previousUpdateId = toNumber(orderbook.lastUpdateId);
    const firstUpdateId = toNumber(json.U);
    const lastUpdateId = toNumber(json.u);
    if (previousUpdateId && firstUpdateId > previousUpdateId + 1) {
      this.parent
        .fetchOrderBook(symbol, 200)
        .then((snapshot) => {
          if (!this.isDisposed) {
            this.orderbooks[symbol] = snapshot;
            this.handleRoutedMessage(`depth.${symbol}`, json);
          }
        })
        .catch(() => {});
      return;
    }

    for (const [price, amount] of json.b || []) {
      updateOrderBookByPrice(orderbook, 'bids', toNumber(price), toNumber(amount));
    }
    for (const [price, amount] of json.a || []) {
      updateOrderBookByPrice(orderbook, 'asks', toNumber(price), toNumber(amount));
    }
    orderbook.lastUpdateId = toIntegerString(lastUpdateId || json.u);
    calcOrderBookTotal(orderbook);
    this.handleRoutedMessage(`depth.${symbol}`, json);
  };
}
