import type { Candle, OHLCVOptions, OrderBook, Ticker } from '../../types';
import type {
  CryptocomWsBookUpdate,
  CryptocomWsCandlestickUpdate,
  CryptocomWsResponse,
  CryptocomWsTickerUpdate,
  CryptocomWsTradeUpdate,
} from './cryptocom.types';

import throttle from 'lodash/throttle';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { calcOrderBookTotal, sortOrderBook } from '../../utils/orderbook';
import { updateOrderBook } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { CryptocomExchange } from './cryptocom.exchange';
import { ENDPOINTS, WSS_URLS } from './cryptocom.types';

export class CryptocomPublicWebSocket extends BaseWebSocket<CryptocomExchange> {
  private subscriptions: Set<string> = new Set();
  private messageId = 1;

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
      this.resubscribe();
    }
  };

  private resubscribe = () => {
    if (this.subscriptions.size > 0) {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: {
            channels: Array.from(this.subscriptions),
          },
          nonce: Date.now(),
        }),
      );
    }
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
          nonce: Date.now(),
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
          nonce: Date.now(),
        }),
      );
    }
  };

  // Batched subscribe used at startup. One subscribe message per chunk of
  // SUBSCRIBE_CHUNK channels — Crypto.com closes the connection with 40107
  // "Maximum subscriptions exceeded" if you fire one message per market for
  // 250+ perp tickers.
  subscribeToTickers = (symbols: string[]) => {
    const SUBSCRIBE_CHUNK = 100;
    const newChannels: string[] = [];
    for (const symbol of symbols) {
      const channel = `ticker.${symbol}`;
      if (!this.subscriptions.has(channel)) {
        this.subscriptions.add(channel);
        newChannels.push(channel);
      }
    }
    for (let i = 0; i < newChannels.length; i += SUBSCRIBE_CHUNK) {
      const chunk = newChannels.slice(i, i + SUBSCRIBE_CHUNK);
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: { channels: chunk },
          nonce: Date.now(),
        }),
      );
    }
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
          nonce: Date.now(),
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
          nonce: Date.now(),
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

  private handleBookUpdate = throttle(
    (update: CryptocomWsBookUpdate) => {
      try {
        // Use O(1) hashmap lookup instead of O(n) find
        const market = this.store.marketsById[update.instrument_name];
        if (!market) return;

        const data = update.data[0];
        if (!data || data.bids.length === 0 || data.asks.length === 0) return;

        // Update best bid/ask in ticker
        // Use O(1) hashmap lookup instead of O(n) find
        const ticker = this.store.tickersBySymbol[market.symbol];
        if (ticker) {
          this.store.updateTicker(ticker, {
            bid: parseFloat(data.bids[0][0]),
            ask: parseFloat(data.asks[0][0]),
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
    const handler = update.channel;
    const callbacks = this.messageHandlers[handler] || [];
    callbacks.forEach((callback) => callback(update));
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const channel = `candlestick.${opts.interval}.${opts.symbol}`;
    const handler = `candlestick.${opts.interval}.${opts.symbol}`;

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
        this.subscribeToCandlestick(opts.symbol, opts.interval);
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
    const innerId = nanoid();

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const channel = `book.${symbol}.50`;
    const handler = `book.${symbol}`;
    const orderBook: OrderBook = { bids: [], asks: [] };

    const fetchSnapshot = async () => {
      const { data } = await this.parent.xhr.post(ENDPOINTS.BOOK, {
        instrument_name: symbol,
        depth: 50,
      });

      if (!this.isDisposed) {
        // Save snapshot into orderBook object
        const bookData = data.data?.[0];
        if (bookData) {
          orderBook.bids = bookData.bids.map(([price, quantity]: [string, string, string]) => ({
            price: parseFloat(price),
            amount: parseFloat(quantity),
            total: 0,
          }));

          orderBook.asks = bookData.asks.map(([price, quantity]: [string, string, string]) => ({
            price: parseFloat(price),
            amount: parseFloat(quantity),
            total: 0,
          }));

          // Apply any queued updates
          this.orderBookInnerState[innerId].updates.forEach((update: any) => {
            this.processOrderBookUpdate(orderBook, update);
          });

          sortOrderBook(orderBook);
          calcOrderBookTotal(orderBook);

          this.orderBookInnerState[innerId].isSnapshotLoaded = true;
          this.orderBookInnerState[innerId].updates = [];

          callback(orderBook);
        }
      }
    };

    const parser = (update: CryptocomWsBookUpdate) => {
      if (update.instrument_name !== symbol) return;

      const bookData = update.data[0];
      if (!bookData) return;

      // First update, request snapshot
      if (!this.orderBookInnerState[innerId].isSnapshotLoaded) {
        this.orderBookInnerState[innerId].updates.push(bookData);

        if (this.orderBookInnerState[innerId].updates.length === 1) {
          fetchSnapshot();
        }
        return;
      }

      // Apply update — processOrderBookUpdate now uses updateOrderBook
      // (binary-search sorted insertion), so the trailing sortOrderBook is
      // redundant for the delta path.
      this.processOrderBookUpdate(orderBook, bookData);
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

      delete this.orderBookInnerState[innerId];

      if (this.isConnected && !this.messageHandlers[handler].length) {
        this.unsubscribe([channel]);
        this.cleanupMessageHandlerStats(handler);
        delete this.messageHandlers[handler];
      }
    };
  };

  private processOrderBookUpdate = (
    orderBook: OrderBook,
    update: { bids: [string, string, string][]; asks: [string, string, string][] },
  ) => {
    // updateOrderBook does binary-search sorted insertion (O(log N) lookup),
    // replacing the prior O(N) findIndex scan per level.
    const bids = update.bids;
    if (bids) {
      for (let i = 0; i < bids.length; i++) {
        const price = parseFloat(bids[i][0]);
        const amount = parseFloat(bids[i][1]);
        updateOrderBook(orderBook, 'bids', { price, amount, total: 0 });
      }
    }
    const asks = update.asks;
    if (asks) {
      for (let i = 0; i < asks.length; i++) {
        const price = parseFloat(asks[i][0]);
        const amount = parseFloat(asks[i][1]);
        updateOrderBook(orderBook, 'asks', { price, amount, total: 0 });
      }
    }
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
          nonce: Date.now(),
        }),
      );

      channels.forEach((channel) => this.subscriptions.delete(channel));
    }
  };

  dispose = () => {
    this.subscriptions.clear();
    if (this.ws) {
      this.ws.close();
    }
  };
}
