import type { Candle, OHLCVOptions, OrderBook, Ticker, Writable } from '../../../types';
import type { KucoinSpotExchange } from './kucoinspot.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { calcOrderBookTotal, sortOrderBook } from '../../../utils/orderbook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { KUCOIN_SPOT_ENDPOINTS, KUCOIN_SPOT_INTERVAL } from './kucoinspot.types';

type Data = Record<string, any>;

const parseNumber = (value: unknown): number | undefined => {
  const parsed = parseFloat(`${value ?? ''}`);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const normalizeTimestamp = (timestamp: unknown): number => {
  const parsed = parseInt(`${timestamp ?? Date.now()}`, 10);
  return parsed > 10_000_000_000 ? Math.floor(parsed / 1000) : parsed;
};

export class KucoinSpotPublicWebsocket extends BaseWebSocket<KucoinSpotExchange> {
  token: string | null = null;
  socketEndpoint: string | null = null;
  pingInterval = 5_000;

  connectAndSubscribe = async () => {
    if (this.isDisposed) return;

    try {
      const { data } = await this.parent.xhr.post(KUCOIN_SPOT_ENDPOINTS.GET_PUBLIC_WS_TOKEN);

      if (data.code !== '200000' || !data.data) {
        this.parent.emitAndLogError(data.msg || 'Failed to get KuCoin Spot public WebSocket token');
        return;
      }

      const server = data.data.instanceServers?.[0];
      if (!server?.endpoint) {
        this.parent.emitAndLogError('No KuCoin Spot public WebSocket servers available');
        return;
      }

      this.token = data.data.token;
      this.pingInterval = server.pingInterval || this.pingInterval;
      this.socketEndpoint = `${server.endpoint}?token=${this.token}&connectId=${Date.now()}`;

      this.ws = this.createWebsocket(this.socketEndpoint);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    } catch (err: any) {
      this.retryFailedConnect(
        err?.response?.data?.msg || err?.message || 'Failed to connect KuCoin Spot public WebSocket',
      );
    }
  };

  onOpen = () => {
    if (this.isDisposed) return;

    this.subscribe();
    this.ping();

    if (ENABLE_RECONNECTING_WEBSOCKET) {
      this.emitWsReconnected();
    }
  };

  ping = () => {
    if (this.isDisposed) return;

    this.pingAt = performance.now();
    this.send(JSON.stringify({ id: Date.now(), type: 'ping' }));
  };

  subscribe = () => {
    this.subscribeTopic('/market/ticker:all');

    for (const topic of Object.keys(this.messageHandlers)) {
      if (topic.startsWith('candle_')) {
        const [symbol, interval] = topic.replace('candle_', '').split('_');
        this.subscribeTopic(`/market/candles:${symbol}_${interval}`);
      }

      if (topic.startsWith('orderBook_')) {
        const symbol = topic.replace('orderBook_', '');
        this.subscribeTopic(`/spotMarket/level2Depth50:${symbol}`);
      }
    }
  };

  private subscribeTopic = (topic: string) => {
    if (!this.isConnected || this.isDisposed) return;

    this.send(
      JSON.stringify({
        id: Date.now(),
        type: 'subscribe',
        topic,
        privateChannel: false,
        response: true,
      }),
    );
  };

  private unsubscribeTopic = (topic: string) => {
    if (!this.isConnected || this.isDisposed) return;

    this.send(
      JSON.stringify({
        id: Date.now(),
        type: 'unsubscribe',
        topic,
        privateChannel: false,
        response: true,
      }),
    );
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;

    this.trackLastReceived();
    const json = jsonParse<Data>(data);
    if (!json) return;

    if (json.type === 'pong') {
      this.handlePongEvent(this.pingInterval);
      return;
    }

    if (json.topic?.startsWith('/market/ticker')) {
      this.recordMessageHandlerStats('ticker');
      this.handleTickerSnapshot(json);
      return;
    }

    if (json.topic?.startsWith('/market/candles')) {
      const topic = this.getCandleHandlerTopic(json.topic);
      this.recordMessageHandlerStats(topic);
      this.messageHandlers[topic]?.forEach((cb) => cb(json));
      return;
    }

    if (json.topic?.startsWith('/spotMarket/level2Depth50')) {
      const symbol = `${json.topic}`.split(':')[1];
      const topic = `orderBook_${symbol}`;
      this.recordMessageHandlerStats(topic);
      this.messageHandlers[topic]?.forEach((cb) => cb(json));
    }
  };

  handleTickerSnapshot = (json: Data) => {
    const data = json.data;
    // On the all-symbols feed (`/market/ticker:all`) the symbol is in
    // `json.subject` (e.g. "BTC-USDT"); `data` has no symbol and the topic is
    // literally ":all". Without reading subject, every ticker update is dropped.
    const symbol = data?.symbol || json.subject || `${json.topic || ''}`.split(':')[1];
    if (!data || !symbol || symbol === 'all') return;

    const ticker = this.parent.store.tickersBySymbol[symbol];
    if (!ticker) return;

    const bid = parseNumber(data.bestBid ?? data.bestBidPrice ?? data.buy);
    const ask = parseNumber(data.bestAsk ?? data.bestAskPrice ?? data.sell);
    const last = parseNumber(data.price ?? data.last ?? data.lastPrice);
    const percentage = parseNumber(data.changeRate);
    const volume = parseNumber(data.volume ?? data.vol ?? data.volume24h);
    const quoteVolume = parseNumber(data.turnover ?? data.volValue ?? data.turnover24h);

    const update: Partial<Writable<Ticker>> = {};
    if (bid !== undefined) update.bid = bid;
    if (ask !== undefined) update.ask = ask;
    if (last !== undefined) {
      update.last = last;
      update.mark = last;
      update.index = last;
    }
    if (percentage !== undefined) update.percentage = percentage * 100;
    if (volume !== undefined) update.volume = volume;
    if (quoteVolume !== undefined) update.quoteVolume = quoteVolume;

    if (Object.keys(update).length > 0) {
      this.parent.store.updateTicker(ticker, update);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const interval = KUCOIN_SPOT_INTERVAL[opts.interval];
    const handlerTopic = `candle_${opts.symbol}_${interval}`;
    const wsTopic = `/market/candles:${opts.symbol}_${interval}`;

    const parser = (message: Data) => {
      const rawCandle = message.data?.candles;
      if (!Array.isArray(rawCandle)) return;

      callback({
        timestamp: normalizeTimestamp(rawCandle[0]),
        open: parseFloat(rawCandle[1]),
        close: parseFloat(rawCandle[2]),
        high: parseFloat(rawCandle[3]),
        low: parseFloat(rawCandle[4]),
        volume: parseFloat(rawCandle[5]),
      });
    };

    this.messageHandlers[handlerTopic] = [...(this.messageHandlers[handlerTopic] || []), parser];
    this.subscribeTopic(wsTopic);

    return () => {
      this.messageHandlers[handlerTopic] = (this.messageHandlers[handlerTopic] || []).filter((f) => f !== parser);

      if (this.messageHandlers[handlerTopic].length === 0) {
        delete this.messageHandlers[handlerTopic];
        this.cleanupMessageHandlerStats(handlerTopic);
        this.unsubscribeTopic(wsTopic);
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const handlerTopic = `orderBook_${symbol}`;
    const wsTopic = `/spotMarket/level2Depth50:${symbol}`;

    const parser = (message: Data) => {
      const data = message.data;
      if (!data) return;

      const orderBook: OrderBook = {
        bids: (data.bids || [])
          .map(([price, amount]: [string, string]) => ({
            price: parseFloat(price),
            amount: parseFloat(amount),
            total: 0,
          }))
          .filter(
            ({ price, amount }: { price: number; amount: number }) => Number.isFinite(price) && Number.isFinite(amount),
          ),
        asks: (data.asks || [])
          .map(([price, amount]: [string, string]) => ({
            price: parseFloat(price),
            amount: parseFloat(amount),
            total: 0,
          }))
          .filter(
            ({ price, amount }: { price: number; amount: number }) => Number.isFinite(price) && Number.isFinite(amount),
          ),
      };

      sortOrderBook(orderBook);
      calcOrderBookTotal(orderBook);
      callback(orderBook);
    };

    this.messageHandlers[handlerTopic] = [...(this.messageHandlers[handlerTopic] || []), parser];
    this.subscribeTopic(wsTopic);

    return () => {
      this.messageHandlers[handlerTopic] = (this.messageHandlers[handlerTopic] || []).filter((f) => f !== parser);

      if (this.messageHandlers[handlerTopic].length === 0) {
        delete this.messageHandlers[handlerTopic];
        this.cleanupMessageHandlerStats(handlerTopic);
        this.unsubscribeTopic(wsTopic);
      }
    };
  };

  private getCandleHandlerTopic = (topic: string) => {
    const [, market] = topic.split(':');
    const [symbol, interval] = market.split('_');
    return `candle_${symbol}_${interval}`;
  };
}
