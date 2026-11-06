import type { Candle, OHLCVOptions, OrderBook, Ticker, Writable } from '../../../types';
import { resolveTransportRouting } from '../../../transport/registry';
import type { BitgetExchange } from './bitget.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { updateOrderBook } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, INTERVAL } from '../../../exchanges/bitget/bitget.types';


type Data = Record<string, any>;

type SubscribedTopics = {
  [id: string]: { [instId: string]: number };
};

type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class BitgetPublicWebsocket extends BaseWebSocket<BitgetExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};
  private messageCount = 0;
  private useFallbackUrl = false;
  private lastMessageCount = 0;
  private connectionAttempts = 0;
  private readonly MAX_ATTEMPTS = 2;

  private toFiniteNumber = (value: unknown): number | undefined => {
    if (typeof value === 'number') {
      return Number.isFinite(value) ? value : undefined;
    }
    if (typeof value === 'string') {
      if (value.trim() === '') return undefined;
      const parsed = Number.parseFloat(value);
      return Number.isFinite(parsed) ? parsed : undefined;
    }
    return undefined;
  };

  private buildTickerUpdate = (data: Record<string, any>): Partial<Writable<Ticker>> => {
    const update: Partial<Writable<Ticker>> = {};

    const bid = this.toFiniteNumber(data.bidPr ?? data.bidPrice ?? data.bid);
    if (bid !== undefined) update.bid = bid;

    const ask = this.toFiniteNumber(data.askPr ?? data.askPrice ?? data.ask);
    if (ask !== undefined) update.ask = ask;

    const last = this.toFiniteNumber(data.lastPr ?? data.lastPrice ?? data.last);
    if (last !== undefined) update.last = last;

    const mark = this.toFiniteNumber(data.markPrice ?? data.markPr ?? data.mark);
    if (mark !== undefined) update.mark = mark;

    const index = this.toFiniteNumber(data.indexPrice ?? data.indexPr ?? data.index);
    if (index !== undefined) update.index = index;

    const change = this.toFiniteNumber(data.change24h ?? data.changeUtc24h ?? data.change);
    if (change !== undefined) update.percentage = change * 100;

    const fundingRate = this.toFiniteNumber(data.fundingRate);
    if (fundingRate !== undefined) update.fundingRate = fundingRate;

    const fundingTime = this.toFiniteNumber(data.nextFundingTime ?? data.fundingTime);
    if (fundingTime !== undefined) update.fundingTime = fundingTime;

    const volume = this.toFiniteNumber(data.baseVolume ?? data.volume);
    if (volume !== undefined) update.volume = volume;

    const quoteVolume = this.toFiniteNumber(data.quoteVolume);
    if (quoteVolume !== undefined) update.quoteVolume = quoteVolume;

    return update;
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      // Check if we need to switch to fallback URL
      if (this.lastMessageCount === 0 && this.messageCount === 0) {
        this.connectionAttempts++;
        if (this.connectionAttempts >= this.MAX_ATTEMPTS) {
          this.useFallbackUrl = true;
        }
      }
      this.lastMessageCount = this.messageCount;
      this.messageCount = 0; // Reset message counter on new connection

      this.topics.ticker = this.parent.store.markets
        .filter((_, i) => i % 3 === 0)
        .reduce(
          (acc, m) => ({
            ...acc,
            [m.symbol]: 1,
          }),
          {},
        );

      // Only a transport that declares a fallback host has one to switch to.
      const fallbackUrl = resolveTransportRouting(this.parent.name, this.parent.options).wsPublic.fallback;
      const useFallbackUrl = this.useFallbackUrl && !!fallbackUrl;
      const url = useFallbackUrl ? fallbackUrl : BASE_WS_URL;
      if (useFallbackUrl) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bitget proxy (public)');
        } catch (_e) {}
      }
      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.subscribe();
      this.ping();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  private onMarketsChanged = ({ added }: { added: Array<{ symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    // Only subscribe to 1/3 of new markets (index % 3 === 0), matching connectAndSubscribe pattern
    const existingMarketCount = Object.keys(this.topics.ticker || {}).length;
    const newArgs = added
      .filter((_, i) => (existingMarketCount + i) % 3 === 0)
      .map((m) => ({
        instType: this.parent.apiProductType.toUpperCase(),
        channel: 'ticker',
        instId: m.symbol,
      }));

    if (newArgs.length === 0) return;

    for (const arg of newArgs) {
      if (!this.topics.ticker) this.topics.ticker = {};
      this.topics.ticker[arg.instId] = 1;
    }

    this.send(JSON.stringify({ op: 'subscribe', args: newArgs }));
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send('ping');
    }
  };

  subscribe = () => {
    const args = Object.entries(this.topics).flatMap(([channel, symbols]) =>
      Object.keys(symbols).map((instId) => ({
        instType: this.parent.apiProductType.toUpperCase(),
        channel,
        instId,
      })),
    );

    this.send(JSON.stringify({ op: 'subscribe', args }));
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.messageCount++; // Increment message counter

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (data.includes('"channel":"ticker"')) {
        const json = jsonParse<Record<string, any>>(data);
        if (json?.arg?.channel === 'ticker' && json?.event !== 'subscribe') {
          this.recordMessageHandlerStats('ticker');
          this.handleTickerTopic(json);
        }
        return;
      }

      if (data.includes('"action":"update"') && data.includes('"channel":"candle')) {
        const json = jsonParse(data);

        if (json) {
          const interval = json.arg.channel.replace('candle', '');
          const topic = `candle_${json.arg.instId}_${interval}`;

          this.recordMessageHandlerStats(topic);

          if (this.messageHandlers[topic]) {
            for (const cb of this.messageHandlers[topic]) {
              cb(json);
            }
          }
        }
      }

      if (data.includes('"channel":"books"')) {
        const json = jsonParse(data);

        if (json) {
          const topic = `orderBook_${json.arg.instId}`;

          this.recordMessageHandlerStats(topic);

          if (this.messageHandlers[topic]) {
            for (const cb of this.messageHandlers[topic]) {
              cb(json);
            }
          }
        }
      }
    }
  };

  handleTickerTopic = (json: Record<string, any>) => {
    const dataList = Array.isArray(json?.data) ? json.data : [];
    for (const data of dataList) {
      const symbol = `${data?.instId || data?.symbol || ''}`;
      if (!symbol) continue;

      const ticker = this.parent.store.tickersBySymbol[symbol];
      if (!ticker) continue;

      const update = this.buildTickerUpdate(data);
      if (Object.keys(update).length === 0) continue;

      this.parent.store.updateTicker(ticker, update);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const interval = INTERVAL[opts.interval];
    const topic = `candle_${opts.symbol}_${interval}`;
    const channel = `candle${interval}`;
    let timeoutId: NodeJS.Timeout | null = null;
    let isActive = true;
    let subscribed = false;

    const parser = (data: Data) => {
      const c = Array.isArray(data?.data) ? data.data[0] : undefined;
      if (!Array.isArray(c)) return;

      const candle: Candle = {
        timestamp: parseInt(c[0], 10) / 1000,
        open: parseFloat(c[1]),
        high: parseFloat(c[2]),
        low: parseFloat(c[3]),
        close: parseFloat(c[4]),
        volume: parseFloat(c[6]),
      };
      callback(candle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (!isActive || this.isDisposed) return;

      if (this.isConnected) {
        this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];

        const payload = {
          op: 'subscribe',
          args: [
            {
              instType: this.parent.apiProductType.toUpperCase(),
              channel,
              instId: opts.symbol,
            },
          ],
        };

        if (!this.topics[channel]) {
          this.topics[channel] = {};
        }
        this.topics[channel][opts.symbol] = (this.topics[channel][opts.symbol] || 0) + 1;
        subscribed = true;

        this.send(JSON.stringify(payload));
        //this.parent.log(`Switched to [${opts.symbol}:${opts.interval}]`);
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      if (!isActive) return;
      isActive = false;

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      // Keep the key with an empty array rather than deleting it. `base.ws.ts`
      // -> `recordMessageHandlerStats` falls back to a count of 1 for an absent
      // handler key, so a deleted key reads as a live subscription; an empty one
      // reads as 0. Resubscribe is driven off `this.topics`, not this map.
      // Unconditional, so a teardown that runs before the subscription ever
      // connected now materializes an empty key where the old `delete` was a
      // no-op — inert, since no stats entry exists without a frame.
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      if (!subscribed) return;
      subscribed = false;

      const nextTopicCount = (this.topics[channel]?.[opts.symbol] || 0) - 1;
      if (nextTopicCount > 0) {
        this.topics[channel][opts.symbol] = nextTopicCount;
      } else {
        delete this.topics[channel]?.[opts.symbol];
        if (Object.keys(this.topics[channel] || {}).length === 0) {
          delete this.topics[channel];
        }
      }

      if (this.isConnected && nextTopicCount <= 0) {
        this.send(
          JSON.stringify({
            op: 'unsubscribe',
            args: [
              {
                instType: this.parent.apiProductType.toUpperCase(),
                channel: `candle${interval}`,
                instId: opts.symbol,
              },
            ],
          }),
        );
        this.cleanupMessageHandlerStats(topic);
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    let isActive = true;
    let subscribed = false;

    const orderBook: OrderBook = { bids: [], asks: [] };
    const channel = 'books';

    const topic = `orderBook_${symbol}`;

    const parser = (data: Data) => {
      const row = Array.isArray(data?.data) ? data.data[0] : undefined;
      if (!row || typeof row !== 'object') return;

      if (data.action === 'snapshot') {
        orderBook.bids = [];
        orderBook.asks = [];

        const bids = row.bids;
        if (bids) {
          for (let i = 0; i < bids.length; i++) {
            const o = bids[i];
            orderBook.bids.push({
              price: parseFloat(o[0]),
              amount: parseFloat(o[1]),
              total: 0,
            });
          }
        }
        const asks = row.asks;
        if (asks) {
          for (let i = 0; i < asks.length; i++) {
            const o = asks[i];
            orderBook.asks.push({
              price: parseFloat(o[0]),
              amount: parseFloat(o[1]),
              total: 0,
            });
          }
        }
      }

      if (data.action === 'update') {
        const bids = row.bids;
        if (bids) {
          for (let i = 0; i < bids.length; i++) {
            const o = bids[i];
            updateOrderBook(orderBook, 'bids', {
              price: parseFloat(o[0]),
              amount: parseFloat(o[1]),
              total: 0,
            });
          }
        }
        const asks = row.asks;
        if (asks) {
          for (let i = 0; i < asks.length; i++) {
            const o = asks[i];
            updateOrderBook(orderBook, 'asks', {
              price: parseFloat(o[0]),
              amount: parseFloat(o[1]),
              total: 0,
            });
          }
        }
      }

      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (!isActive || this.isDisposed) return;

      if (this.isConnected) {
        this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];

        const payload = {
          op: 'subscribe',
          args: [
            {
              instType: this.parent.apiProductType.toUpperCase(),
              channel,
              instId: symbol,
            },
          ],
        };

        if (!this.topics[channel]) {
          this.topics[channel] = {};
        }
        this.topics[channel][symbol] = (this.topics[channel][symbol] || 0) + 1;
        subscribed = true;

        this.send(JSON.stringify(payload));
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      if (!isActive) return;
      isActive = false;

      // Keep the key with an empty array rather than deleting it. `base.ws.ts`
      // -> `recordMessageHandlerStats` falls back to a count of 1 for an absent
      // handler key, so a deleted key reads as a live subscription; an empty one
      // reads as 0. Resubscribe is driven off `this.topics`, not this map.
      // Unconditional, so a teardown that runs before the subscription ever
      // connected now materializes an empty key where the old `delete` was a
      // no-op — inert, since no stats entry exists without a frame.
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      orderBook.asks = [];
      orderBook.bids = [];

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (!subscribed) return;
      subscribed = false;

      const nextTopicCount = (this.topics[channel]?.[symbol] || 0) - 1;
      if (nextTopicCount > 0) {
        this.topics[channel][symbol] = nextTopicCount;
      } else {
        delete this.topics[channel]?.[symbol];
        if (Object.keys(this.topics[channel] || {}).length === 0) {
          delete this.topics[channel];
        }
      }

      if (this.isConnected && nextTopicCount <= 0) {
        const payload = {
          op: 'unsubscribe',
          args: [
            {
              instType: this.parent.apiProductType.toUpperCase(),
              channel: 'books',
              instId: symbol,
            },
          ],
        };

        this.cleanupMessageHandlerStats(topic);
        this.send(JSON.stringify(payload));
      }
    };
  };
}
