import type { Candle, OHLCVOptions, OrderBook, Timeframe } from '../../../types';
import type { KrakenSpotExchange } from './krakenspot.exchange';

import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, INTERVAL } from './krakenspot.types';

// Kraken Spot WebSocket v2 protocol:
// Subscribe: { method: "subscribe", params: { channel: "ticker", symbol: ["BTC/USD"] } }
// Messages: { channel: "ticker", type: "snapshot"|"update", data: [{...}] }

export class KrakenSpotPublicWebsocket extends BaseWebSocket<KrakenSpotExchange> {
  private ohlcvHandlers: { [key: string]: Array<(c: Candle) => void> } = {};
  private orderBookHandlers: { [symbol: string]: Array<(o: OrderBook) => void> } = {};
  private orderBookCache: { [symbol: string]: { bids: Map<number, number>; asks: Map<number, number> } } = {};

  private wireSymbol = (symbol: string) => this.store.marketsBySymbol[symbol]?.id ?? symbol;
  private sdkSymbol = (symbol: string) => this.store.marketsById[symbol]?.symbol ?? symbol;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const url = BASE_WS_URL.public.livenet;
      this.ws = this.createWebsocket(url);

      const handleClose = () => {
        this.onClose();
      };

      const handleError = () => {
        try {
          if (this.ws && this.ws.readyState !== WebSocket.CLOSED) this.ws.close();
        } catch (_e) {}
        handleClose();
      };

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage as any);
      this.ws.addEventListener('close', handleClose);
      this.ws.addEventListener('error', handleError);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      // REST wsname retains XBT; v2 uses normalized asset names such as BTC.
      this.orderBookCache = {};
      const symbols = this.parent.store.markets.map((m) => this.wireSymbol(m.symbol));

      // Batch in groups of 50
      for (let i = 0; i < symbols.length; i += 50) {
        const batch = symbols.slice(i, i + 50);
        this.send(
          JSON.stringify({
            method: 'subscribe',
            params: {
              channel: 'ticker',
              symbol: batch,
              event_trigger: 'trades',
            },
          }),
        );
      }

      // Resubscribe to active orderbook handlers
      for (const symbol of Object.keys(this.orderBookHandlers)) {
        if (this.orderBookHandlers[symbol]?.length > 0) {
          this.send(
            JSON.stringify({
              method: 'subscribe',
              params: {
                channel: 'book',
                symbol: [this.wireSymbol(symbol)],
                depth: 100,
              },
            }),
          );
        }
      }

      // Resubscribe to active OHLCV handlers
      for (const key of Object.keys(this.ohlcvHandlers)) {
        if (this.ohlcvHandlers[key]?.length > 0) {
          const [symbol, tf] = key.split('::');
          const interval = INTERVAL[tf as Timeframe] || 1;
          this.send(
            JSON.stringify({
              method: 'subscribe',
              params: {
                channel: 'ohlc',
                symbol: [this.wireSymbol(symbol)],
                interval: interval,
              },
            }),
          );
        }
      }
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      try {
        const json = typeof data === 'string' ? JSON.parse(data) : JSON.parse((data as any).toString?.() || '{}');

        // WS v2 responses have method field for sub/unsub confirmations
        if (json?.method) {
          if (json.method === 'subscribe' && !json.success) {
            console.error('[KrakenSpot WS] Subscription error:', json.error);
          }
          return;
        }

        // Heartbeat
        if (json?.channel === 'heartbeat') {
          return;
        }

        const channel = json?.channel;
        if (!channel) return;

        // Record stats for ongoing feeds. `book` and `ohlc` are recorded in
        // their handlers instead, AFTER the no-listeners guard: this venue keeps
        // subscriptions in private handler maps, so those stats keys match no
        // `messageHandlers` key and `reconcileMessageHandlerStats` can never
        // mark them idle. Recording here would also resurrect the entry from
        // frames still in flight between the disposer's cleanup and the
        // server's unsubscribe ack.
        if (json.type !== 'snapshot' && channel !== 'book' && channel !== 'ohlc') {
          this.recordMessageHandlerStats(channel);
        }

        switch (channel) {
          case 'ticker':
            this.handleTicker(json);
            break;
          case 'book':
            this.handleBook(json);
            break;
          case 'ohlc':
            this.handleOHLC(json);
            break;
        }
      } catch (e) {
        console.error('krakenspot.ws-public parse error', e, 'Raw data:', data);
      }
    }
  };

  private handleTicker = (msg: any) => {
    // WS v2 format: { channel: "ticker", type: "snapshot"|"update", data: [{ symbol, bid, bid_qty, ask, ask_qty, last, volume, vwap, high, low, change, change_pct }] }
    const items = msg.data;
    if (!Array.isArray(items)) return;

    for (const item of items) {
      const symbol = this.sdkSymbol(item.symbol);
      if (!symbol) continue;

      const ticker = this.parent.store.tickersBySymbol[symbol];
      if (!ticker) continue;

      const num = (x: any) => (x === undefined || x === null ? undefined : parseFloat(x));
      const changes: any = {};

      const bid = num(item.bid);
      const ask = num(item.ask);
      const last = num(item.last);
      const volume = num(item.volume);
      const percentage = num(item.change_pct);

      if (bid !== undefined) changes.bid = bid;
      if (ask !== undefined) changes.ask = ask;
      if (last !== undefined) {
        changes.last = last;
        // Spot has no separate mark/index price; use last as mark
        changes.mark = last;
        changes.index = last;
      }
      if (volume !== undefined) changes.volume = volume;
      if (percentage !== undefined) changes.percentage = percentage;

      this.store.updateTicker(ticker, changes);
    }
  };

  private handleBook = (msg: any) => {
    // WS v2 format: { channel: "book", type: "snapshot"|"update", data: [{ symbol, bids: [{price, qty}], asks: [{price, qty}], checksum }] }
    const items = msg.data;
    if (!Array.isArray(items)) return;

    for (const item of items) {
      const symbol = this.sdkSymbol(item.symbol);
      if (!symbol) continue;

      if (msg.type === 'snapshot') {
        this.handleBookSnapshot(symbol, item);
      } else {
        this.handleBookDelta(symbol, item);
      }
    }
  };

  private handleBookSnapshot = (symbol: string, item: any) => {
    const handlers = this.orderBookHandlers[symbol] || [];
    if (handlers.length === 0) return;

    // Clear and rebuild cache
    this.orderBookCache[symbol] = {
      bids: new Map(),
      asks: new Map(),
    };

    const rawBids = item.bids || [];
    const rawAsks = item.asks || [];

    for (const bid of rawBids) {
      const price = parseFloat(bid.price);
      const qty = parseFloat(bid.qty);
      if (qty > 0) {
        this.orderBookCache[symbol].bids.set(price, qty);
      }
    }

    for (const ask of rawAsks) {
      const price = parseFloat(ask.price);
      const qty = parseFloat(ask.qty);
      if (qty > 0) {
        this.orderBookCache[symbol].asks.set(price, qty);
      }
    }

    this.emitOrderBook(symbol);
  };

  private handleBookDelta = (symbol: string, item: any) => {
    const handlers = this.orderBookHandlers[symbol] || [];
    if (handlers.length === 0) return;

    this.recordMessageHandlerStats('book');

    if (!this.orderBookCache[symbol]) return;

    const rawBids = item.bids || [];
    const rawAsks = item.asks || [];

    for (const bid of rawBids) {
      const price = parseFloat(bid.price);
      const qty = parseFloat(bid.qty);
      if (qty === 0) {
        this.orderBookCache[symbol].bids.delete(price);
      } else {
        this.orderBookCache[symbol].bids.set(price, qty);
      }
    }

    for (const ask of rawAsks) {
      const price = parseFloat(ask.price);
      const qty = parseFloat(ask.qty);
      if (qty === 0) {
        this.orderBookCache[symbol].asks.delete(price);
      } else {
        this.orderBookCache[symbol].asks.set(price, qty);
      }
    }

    this.emitOrderBook(symbol);
  };

  private emitOrderBook = (symbol: string) => {
    const handlers = this.orderBookHandlers[symbol] || [];
    if (handlers.length === 0) return;

    const cache = this.orderBookCache[symbol];
    if (!cache) return;

    // Kraken omits deletions for levels that fall outside subscribed depth.
    const bidsSorted = [...cache.bids].sort((a, b) => b[0] - a[0]);
    const asksSorted = [...cache.asks].sort((a, b) => a[0] - b[0]);
    for (const [price] of bidsSorted.slice(100)) cache.bids.delete(price);
    for (const [price] of asksSorted.slice(100)) cache.asks.delete(price);
    const bids: OrderBook['bids'] = bidsSorted.slice(0, 100).map(([price, amount]) => ({ price, amount, total: 0 }));
    const asks: OrderBook['asks'] = asksSorted.slice(0, 100).map(([price, amount]) => ({ price, amount, total: 0 }));

    handlers.forEach((cb) => cb({ bids, asks }));
  };

  // Kraken Spot WS v2 supports OHLC via WebSocket (unlike Futures)
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const key = `${opts.symbol}::${opts.interval}`;
    this.ohlcvHandlers[key] = [...(this.ohlcvHandlers[key] || []), callback];

    const interval = INTERVAL[opts.interval as Timeframe] || 1;

    if (this.isConnected) {
      this.send(
        JSON.stringify({
          method: 'subscribe',
          params: {
            channel: 'ohlc',
            symbol: [this.wireSymbol(opts.symbol)],
            interval: interval,
          },
        }),
      );
    }

    return () => {
      this.ohlcvHandlers[key] = (this.ohlcvHandlers[key] || []).filter((cb) => cb !== callback);
      this.clearFeedStatsIfIdle('ohlc', this.ohlcvHandlers);

      if (this.ohlcvHandlers[key]?.length === 0 && this.isConnected) {
        this.send(
          JSON.stringify({
            method: 'unsubscribe',
            params: {
              channel: 'ohlc',
              symbol: [this.wireSymbol(opts.symbol)],
              interval: interval,
            },
          }),
        );
      }
    };
  };

  private handleOHLC = (msg: any) => {
    // WS v2 format: { channel: "ohlc", type: "snapshot"|"update", data: [{ symbol, open, high, low, close, vwap, volume, trades, interval_begin, interval, timestamp }] }
    const items = msg.data;
    if (!Array.isArray(items)) return;

    for (const item of items) {
      const symbol = this.sdkSymbol(item.symbol);
      const interval = item.interval;
      if (!symbol || !interval) continue;

      // Find the matching timeframe key
      const tf = Object.entries(INTERVAL).find(([, v]) => v === interval)?.[0] as Timeframe | undefined;
      if (!tf) continue;

      const key = `${symbol}::${tf}`;
      const handlers = this.ohlcvHandlers[key] || [];
      if (handlers.length === 0) continue;

      this.recordMessageHandlerStats('ohlc');

      const candle: Candle = {
        timestamp: new Date(item.interval_begin).getTime() / 1000,
        open: parseFloat(item.open),
        high: parseFloat(item.high),
        low: parseFloat(item.low),
        close: parseFloat(item.close),
        volume: parseFloat(item.volume),
      };

      handlers.forEach((cb) => cb(candle));
    }
  };

  /**
   * Clears the shared stats entry for a feed once no symbol is listening.
   *
   * This venue keeps its subscriptions in private `orderBookHandlers` /
   * `ohlcvHandlers` maps and never touches `messageHandlers`, so the bare feed
   * name it records stats under matches no handler key. That makes it invisible
   * to `reconcileMessageHandlerStats` in base.ws.ts, and the entry would keep
   * its final timestamp, age past the staleness threshold and report a dead feed
   * on a healthy socket. Clearing it returns the topic to the harmless
   * never-recorded case.
   *
   * Pairs with recording the feed inside its handler, after the no-listeners
   * guard: without that, a frame arriving right after this call puts the entry
   * straight back — and with `hadHandlers` unset, permanently beyond reconcile.
   */
  private clearFeedStatsIfIdle = (feed: string, handlers: { [key: string]: Array<unknown> }) => {
    for (const key in handlers) {
      if (handlers[key]?.length) return;
    }
    this.cleanupMessageHandlerStats(feed);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    this.orderBookHandlers[symbol] = [...(this.orderBookHandlers[symbol] || []), callback];

    if (this.isConnected) {
      this.send(
        JSON.stringify({
          method: 'subscribe',
          params: {
            channel: 'book',
            symbol: [this.wireSymbol(symbol)],
            depth: 100,
          },
        }),
      );
    } else {
      const sendWhenReady = () => {
        if (this.isConnected) {
          this.send(
            JSON.stringify({
              method: 'subscribe',
              params: {
                channel: 'book',
                symbol: [this.wireSymbol(symbol)],
                depth: 100,
              },
            }),
          );
        } else {
          setTimeout(sendWhenReady, 100);
        }
      };
      setTimeout(sendWhenReady, 100);
    }

    return () => {
      this.orderBookHandlers[symbol] = (this.orderBookHandlers[symbol] || []).filter((cb) => cb !== callback);
      this.clearFeedStatsIfIdle('book', this.orderBookHandlers);

      if (this.orderBookHandlers[symbol]?.length === 0 && this.isConnected) {
        this.send(
          JSON.stringify({
            method: 'unsubscribe',
            params: {
              channel: 'book',
              symbol: [this.wireSymbol(symbol)],
              depth: 100,
            },
          }),
        );
      }
    };
  };
}
