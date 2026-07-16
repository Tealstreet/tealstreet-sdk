import type { Candle, OHLCVOptions, OrderBook, Ticker } from '../../types';
import type {
  CryptocomWsBookUpdate,
  CryptocomWsCandlestickUpdate,
  CryptocomWsResponse,
  CryptocomWsTickerUpdate,
  CryptocomWsTradeUpdate,
  CryptocomWsValuationUpdate,
} from './cryptocom.types';

import throttle from 'lodash/throttle';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { calcOrderBookTotal, sortOrderBook } from '../../utils/orderbook';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { CryptocomExchange } from './cryptocom.exchange';
import { mapCandlestickInterval, WSS_URLS } from './cryptocom.types';

const SUBSCRIBE_CHUNK_SIZE = 100;

export type CryptocomMarketDataSubscription = {
  instrumentName: string;
  indexInstrumentName?: string;
};

export class CryptocomPublicWebSocket extends BaseWebSocket<CryptocomExchange> {
  private subscriptions: Set<string> = new Set();
  private marketIdsByIndexInstrument = new Map<string, Set<string>>();
  private messageId = 1;
  private reconnectSubscriptionTimer?: ReturnType<typeof setTimeout>;

  constructor(parent: CryptocomExchange) {
    super(parent);
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const urls = WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION'];
      this.ws = this.createWebsocket(urls.MARKET);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      clearTimeout(this.reconnectSubscriptionTimer);
      if (this.subscriptions.size > 0) {
        this.reconnectSubscriptionTimer = setTimeout(() => {
          if (!this.isDisposed) this.resubscribe();
        }, 1000);
      }
    }
  };

  private resubscribe = () => {
    if (this.subscriptions.size > 0) {
      this.sendSubscriptionFrames(Array.from(this.subscriptions));
    }
  };

  private sendSubscriptionFrames = (channels: string[]) => {
    for (let i = 0; i < channels.length; i += SUBSCRIBE_CHUNK_SIZE) {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: { channels: channels.slice(i, i + SUBSCRIBE_CHUNK_SIZE) },
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );
    }
  };

  private registerAndSubscribe = (channels: string[]) => {
    const newChannels: string[] = [];
    for (const channel of channels) {
      if (this.subscriptions.has(channel)) continue;
      this.subscriptions.add(channel);
      newChannels.push(channel);
    }
    this.sendSubscriptionFrames(newChannels);
  };

  subscribeToOrderBook = (symbol: string, depth = 50) => {
    const channel = `book.${symbol}.${depth}`;
    if (!this.subscriptions.has(channel)) {
      this.subscriptions.add(channel);
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: {
            channels: [channel],
          },
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );
    }
  };

  subscribeToTicker = (symbol: string) => {
    const channel = `ticker.${symbol}`;
    if (!this.subscriptions.has(channel)) {
      this.subscriptions.add(channel);
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: {
            channels: [channel],
          },
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );
    }
  };

  // Batched subscribe used at startup. 100 channels is a conservative local
  // frame size; Crypto.com documents a 100 market-data request/second limit,
  // but does not document a per-frame channel count or total connection cap.
  subscribeToTickers = (symbols: string[]) => {
    this.registerAndSubscribe(symbols.map((symbol) => `ticker.${symbol}`));
  };

  subscribeToMarketData = (markets: CryptocomMarketDataSubscription[]) => {
    const channels = markets.flatMap(({ instrumentName, indexInstrumentName }) => {
      const marketChannels = [`ticker.${instrumentName}`, `mark.${instrumentName}`, `funding.${instrumentName}`];
      if (!indexInstrumentName) return marketChannels;

      const marketIds = this.marketIdsByIndexInstrument.get(indexInstrumentName) || new Set<string>();
      marketIds.add(instrumentName);
      this.marketIdsByIndexInstrument.set(indexInstrumentName, marketIds);
      marketChannels.splice(2, 0, `index.${indexInstrumentName}`);
      return marketChannels;
    });

    this.registerAndSubscribe(channels);
  };

  subscribeToTrades = (symbol: string) => {
    const channel = `trade.${symbol}`;
    if (!this.subscriptions.has(channel)) {
      this.subscriptions.add(channel);
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: {
            channels: [channel],
          },
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );
    }
  };

  subscribeToCandlestick = (symbol: string, interval: string) => {
    const channel = `candlestick.${interval}.${symbol}`;
    if (!this.subscriptions.has(channel)) {
      this.subscriptions.add(channel);
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: {
            channels: [channel],
          },
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        if (json.method === 'public/heartbeat' && typeof json.id === 'number') {
          this.send(JSON.stringify({ id: json.id, method: 'public/respond-heartbeat' }));
          return;
        }

        // Data frames carry `result.channel` ("book", "ticker", "trade",
        // "candlestick"); the per-symbol sub name (e.g. "book.BTCUSD-PERP.50")
        // is in `result.subscription` / `result.instrument_name`. Heartbeats
        // and subscribe-acks carry `code` + `method` and no `result`. The
        // previous shape early-returned on any `'code' in json`, which dropped
        // every data frame because Crypto.com now embeds `code: 0` on data
        // frames as well. Dispatch first, then fall back to the response path.
        if (json.result && json.result.channel) {
          switch (json.result.channel) {
            case 'ticker':
              this.handleTickerUpdate(json.result as CryptocomWsTickerUpdate);
              break;
            case 'trade':
              this.handleTradeUpdate(json.result as CryptocomWsTradeUpdate);
              break;
            case 'book':
            case 'book.update':
              this.handleBookUpdate(json.result as CryptocomWsBookUpdate);
              break;
            case 'candlestick':
              this.handleCandlestickUpdate(json.result as CryptocomWsCandlestickUpdate);
              break;
            case 'mark':
            case 'index':
            case 'funding':
              this.handleValuationUpdate(json.result as CryptocomWsValuationUpdate);
              break;
            case 'estimatedfunding':
              // Ticker exposes the current funding rate, not a distinct next-
              // interval estimate. Do not overwrite `funding` semantics if an
              // externally managed subscription forwards this channel.
              break;
          }
          return;
        }

        // Non-data response (heartbeat, subscribe ack, error). Only surface
        // non-zero codes — `code: 0` here is just an ack.
        if ('code' in json) {
          const response = json as CryptocomWsResponse;
          if (response.code !== 0) {
            this.parent.emitAndLogError(`WebSocket error ${response.code}: ${response.message || 'Unknown error'}`);
          }
          return;
        }
      } catch (error) {
        this.parent.emitAndLogError(`Error parsing WebSocket message: ${error}`);
      }
    }
  };

  private handleTickerUpdate = throttle(
    (update: CryptocomWsTickerUpdate) => {
      try {
        // Use O(1) hashmap lookup instead of O(n) find
        const market = this.store.marketsById[update.instrument_name];
        if (!market) return;

        // Use O(1) hashmap lookup instead of O(n) find
        const ticker = this.store.tickersBySymbol[market.symbol];
        if (!ticker) return;

        const data = update.data[0];
        if (!data) return;

        // v1 API returns numeric fields as strings; parseFloat at the boundary.
        // `a` is the last trade price; `c` is the 24h percentage change as a
        // decimal fraction. The previous code aliased `c` to both `last` and
        // `percentage`, which is why BTCUSD-PERP showed e.g. "65123.4%" in the
        // 24h column.
        const tickerUpdate: Partial<Ticker> = {
          bid: parseFloat(data.b),
          ask: parseFloat(data.k),
          last: parseFloat(data.a),
          percentage: parseFloat(data.c) * 100,
          volume: parseFloat(data.v),
          quoteVolume: parseFloat(data.vv),
          openInterest: data.oi ? parseFloat(data.oi) : 0,
        };

        this.store.updateTicker(ticker, tickerUpdate);
      } catch (error) {
        this.parent.emitAndLogError(`Error handling ticker update: ${error}`);
      }
    },
    1000,
    { leading: true, trailing: true },
  );

  private handleValuationUpdate = (update: CryptocomWsValuationUpdate) => {
    const rawValue = update.data[0]?.v;
    if (typeof rawValue !== 'string' || rawValue.trim() === '') return;
    const value = Number(rawValue);
    if (!Number.isFinite(value)) return;
    if ((update.channel === 'mark' || update.channel === 'index') && value <= 0) return;

    if (update.channel === 'index') {
      for (const marketId of this.getMarketIdsByIndexInstrument(update.instrument_name)) {
        const market = this.store.marketsById[marketId];
        if (!market) continue;

        const ticker = this.store.tickersBySymbol[market.symbol];
        if (ticker) this.store.updateTicker(ticker, { index: value });
      }
      return;
    }

    const market = this.store.marketsById[update.instrument_name];
    if (!market) return;

    const ticker = this.store.tickersBySymbol[market.symbol];
    if (!ticker) return;

    if (update.channel === 'mark') {
      this.store.updateTicker(ticker, { mark: value });
    } else if (update.channel === 'funding') {
      // Crypto.com documents `funding` as the rate settling at the end of the
      // current hour. It is already a decimal rate, matching Ticker semantics.
      this.store.updateTicker(ticker, { fundingRate: value });
    }
  };

  private getMarketIdsByIndexInstrument = (indexInstrumentName: string) => {
    const cached = this.marketIdsByIndexInstrument.get(indexInstrumentName);
    if (cached) return cached;

    const marketIds = new Set(
      this.store.markets
        .filter((market) => market.info?.underlyingSymbol === indexInstrumentName)
        .map((market) => market.id),
    );
    if (marketIds.size > 0) this.marketIdsByIndexInstrument.set(indexInstrumentName, marketIds);
    return marketIds;
  };

  private handleBookUpdate = throttle(
    (update: CryptocomWsBookUpdate) => {
      try {
        // Use O(1) hashmap lookup instead of O(n) find
        const market = this.store.marketsById[update.instrument_name];
        if (!market) return;

        const data = update.data[0];
        if (!data) return;

        // Update best bid/ask in ticker
        // Use O(1) hashmap lookup instead of O(n) find
        const ticker = this.store.tickersBySymbol[market.symbol];
        if (ticker) {
          this.store.updateTicker(ticker, {
            bid: data.bids[0] ? parseFloat(data.bids[0][0]) : 0,
            ask: data.asks[0] ? parseFloat(data.asks[0][0]) : 0,
          });
        }

        // Trigger message handlers for order book updates
        const handler = `book.${update.instrument_name}`;
        const callbacks = this.messageHandlers[handler] || [];
        callbacks.forEach((callback) => callback(update));
      } catch (error) {
        this.parent.emitAndLogError(`Error handling book update: ${error}`);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleTradeUpdate = (_update: CryptocomWsTradeUpdate) => {
    // Handle trade updates if needed for real-time trade feed
  };

  private handleCandlestickUpdate = (update: CryptocomWsCandlestickUpdate) => {
    // Trigger message handlers for candlestick updates
    const handler = update.subscription;
    const callbacks = this.messageHandlers[handler] || [];
    callbacks.forEach((callback) => callback(update));
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const interval = mapCandlestickInterval(opts.interval);
    const channel = `candlestick.${interval}.${opts.symbol}`;
    const handler = channel;

    const parser = (update: CryptocomWsCandlestickUpdate) => {
      if (update.instrument_name === opts.symbol) {
        const data = update.data[0];
        if (data) {
          callback({
            timestamp: data.t,
            open: parseFloat(data.o),
            high: parseFloat(data.h),
            low: parseFloat(data.l),
            close: parseFloat(data.c),
            volume: parseFloat(data.v),
          });
        }
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        // Subscribe to the candlestick channel
        this.subscribeToCandlestick(opts.symbol, interval);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];

      if (this.isConnected && !this.messageHandlers[handler].length) {
        this.unsubscribe([channel]);
        this.cleanupMessageHandlerStats(handler);
        delete this.messageHandlers[handler];
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const channel = `book.${symbol}.50`;
    const handler = `book.${symbol}`;
    const orderBook: OrderBook = { bids: [], asks: [] };

    const parser = (update: CryptocomWsBookUpdate) => {
      if (update.instrument_name !== symbol) return;

      const bookData = update.data[0];
      if (!bookData) return;

      // The default channel mode is SNAPSHOT: every frame is the complete
      // top-of-book. Replace both sides wholesale so levels that disappeared
      // from the next snapshot cannot survive as phantom liquidity.
      orderBook.bids = bookData.bids.map(([price, quantity]) => ({
        price: parseFloat(price),
        amount: parseFloat(quantity),
        total: 0,
      }));
      orderBook.asks = bookData.asks.map(([price, quantity]) => ({
        price: parseFloat(price),
        amount: parseFloat(quantity),
        total: 0,
      }));
      sortOrderBook(orderBook);
      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        // Subscribe to order book
        this.subscribeToOrderBook(symbol, 50);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];

      if (this.isConnected && !this.messageHandlers[handler].length) {
        this.unsubscribe([channel]);
        this.cleanupMessageHandlerStats(handler);
        delete this.messageHandlers[handler];
      }
    };
  };

  unsubscribe = (channels: string[]) => {
    if (channels.length > 0) {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'unsubscribe',
          params: {
            channels,
          },
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );

      channels.forEach((channel) => this.subscriptions.delete(channel));
    }
  };

  dispose = () => {
    clearTimeout(this.reconnectSubscriptionTimer);
    this.subscriptions.clear();
    this.marketIdsByIndexInstrument.clear();
    if (this.ws) {
      this.ws.close();
    }
  };
}
