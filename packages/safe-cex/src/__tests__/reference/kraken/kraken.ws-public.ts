import type { Candle, OHLCVOptions, OrderBook, Timeframe } from '../../../types';
import type { KrakenExchange } from './kraken.exchange';

import { v } from '../../../utils/get-key';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL } from './kraken.types';

export class KrakenPublicWebsocket extends BaseWebSocket<KrakenExchange> {
  private ohlcvHandlers: { [key: string]: Array<(c: Candle) => void> } = {};
  private orderBookHandlers: { [symbol: string]: Array<(o: OrderBook) => void> } = {};
  private orderBookCache: { [symbol: string]: { bids: Map<number, number>; asks: Map<number, number> } } = {};

  private getTickerProductIds = () => {
    const productIds = this.parent.store.markets.map((m) => m.symbol);
    const activeSymbols = this.parent.getActiveTickerSymbols();
    if (activeSymbols.length === 0) return productIds;

    const activeSet = new Set(activeSymbols);
    const filtered = productIds.filter((symbol) => activeSet.has(symbol));
    return filtered.length > 0 ? filtered : productIds;
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const url = BASE_WS_URL.public[key];
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
      // Subscribe to ticker for the active products when the host supplies them.
      const product_ids = this.getTickerProductIds();
      // Kraken expects batches; keep payload size modest
      for (let i = 0; i < product_ids.length; i += 50) {
        const batch = product_ids.slice(i, i + 50);
        const payload = { event: 'subscribe', feed: 'ticker', product_ids: batch } as const;
        this.send(JSON.stringify(payload));
      }

      // Resubscribe to orderbooks that have active handlers
      for (const symbol of Object.keys(this.orderBookHandlers)) {
        if (this.orderBookHandlers[symbol]?.length > 0) {
          const bookPayload = { event: 'subscribe', feed: 'book', product_ids: [symbol] } as const;
          try {
            this.send(JSON.stringify(bookPayload));
          } catch (e) {
            console.error('[Kraken WS] Failed to resubscribe to book for', symbol, e);
          }
        }
      }

      // Resubscribe to OHLCV if there were active subscriptions
      for (const key of Object.keys(this.ohlcvHandlers)) {
        if (this.ohlcvHandlers[key]?.length > 0) {
          // Note: Kraken Futures doesn't support OHLCV via WebSocket
          // This is here for completeness but won't actually work for Kraken Futures
        }
      }
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      try {
        const json = typeof data === 'string' ? JSON.parse(data) : JSON.parse((data as any).toString?.() || '{}');

        const feed = json?.feed || json?.channel;

        // Also check for event field (subscription confirmations)
        if (json?.event) {
          if (json.event === 'error') {
            console.error('[Kraken WS] Subscription error:', json.message || json);
          }
          return;
        }

        if (!feed) {
          return;
        }

        // Only record stats for ongoing feeds, not one-time snapshots. `book` is
        // recorded inside `handleBookDelta` instead, AFTER its no-listeners
        // guard: this venue keeps subscriptions in private handler maps, so the
        // stats key matches no `messageHandlers` key and
        // `reconcileMessageHandlerStats` can never mark it idle. Recording here
        // would also resurrect the entry on the very next frame after the
        // disposer cleared it — and Kraken Futures is never sent an unsubscribe,
        // so those frames keep arriving.
        if (feed !== 'book_snapshot' && feed !== 'book') {
          this.recordMessageHandlerStats(feed);
        }

        if (feed === 'ticker') {
          this.handleTicker(json);
        } else if (feed === 'ohlc') {
          this.handleOHLC(json);
        } else if (feed === 'book_snapshot') {
          this.handleBookSnapshot(json);
        } else if (feed === 'book') {
          this.handleBookDelta(json);
        }
      } catch (e) {
        console.error('kraken.ws-public parse error', e, 'Raw data:', data);
      }
    }
  };

  handleTicker = (msg: any) => {
    const symbol = v(msg, 'product_id') || v(msg, 'symbol');
    if (!symbol) return;
    const ticker = this.parent.store.tickersBySymbol[symbol];
    if (!ticker) return;

    const num = (x: any) => (x === undefined || x === null ? undefined : parseFloat(x));
    const changes: any = {};

    const bid = num(v(msg, 'bid')) ?? num(v(msg, 'best_bid'));
    const ask = num(v(msg, 'ask')) ?? num(v(msg, 'best_ask'));
    const last = num(v(msg, 'last')) ?? num(v(msg, 'price')) ?? num(v(msg, 'last_trade'));
    const mark = num(v(msg, 'mark')) ?? num(v(msg, 'markPrice'));
    const index = num(v(msg, 'index')) ?? num(v(msg, 'indexPrice'));
    // IMPORTANT: Kraken's `funding_rate` / `fundingRate` fields are the
    // quote currency amount paid per contract (e.g. 0.207 USD per
    // PF_XBTUSD contract), NOT a fractional rate like other exchanges
    // publish. The matching fractional field is `relative_funding_rate`
    // — that's what Tealstreet's UI formats as a percentage and what
    // we want to display. Using the wrong field renders BTC funding as
    // "20.7%" instead of "0.0006%".
    const fundingRate = num(v(msg, 'relative_funding_rate'));
    const fundingTime =
      num(v(msg, 'next_funding_rate_time')) ?? num(v(msg, 'next_funding')) ?? num(v(msg, 'nextFundingTime'));

    // Calculate percentage change if open24h is available
    const open24h = num(v(msg, 'open24h')) ?? num(v(msg, 'open_24h'));
    let percentage: number | undefined;
    if (open24h !== undefined && last !== undefined && open24h > 0) {
      percentage = ((last - open24h) / open24h) * 100;
    }

    if (bid !== undefined) changes.bid = bid;
    if (ask !== undefined) changes.ask = ask;
    if (last !== undefined) changes.last = last;
    if (mark !== undefined) changes.mark = mark;
    if (index !== undefined) changes.index = index;
    if (fundingRate !== undefined) changes.fundingRate = fundingRate;
    if (fundingTime !== undefined) changes.fundingTime = fundingTime as any;
    if (percentage !== undefined) changes.percentage = percentage;

    this.store.updateTicker(ticker, changes);
  };

  // Public OHLCV - Note: Kraken Futures doesn't support OHLC via WebSocket
  // We return a no-op since OHLC is only available via REST API
  listenOHLCV = (_opts: OHLCVOptions, _callback: (candle: Candle) => void) => {
    // Kraken Futures doesn't provide OHLC data via WebSocket
    // Charts/candles are only available via REST API
    // Return empty unsubscribe function
    return () => {};
  };

  // Unused - Kraken Futures doesn't support OHLC via WebSocket
  private handleOHLC = (_msg: any) => {
    // This handler is not used since Kraken Futures doesn't provide OHLC via WebSocket
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

  // Public OrderBook (lightweight: emits snapshots per message)
  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    this.orderBookHandlers[symbol] = [...(this.orderBookHandlers[symbol] || []), callback];
    const payload = { event: 'subscribe', feed: 'book', product_ids: [symbol] } as const;

    // Check if WebSocket is connected, if not, wait for connection
    if (this.isConnected) {
      try {
        this.send(JSON.stringify(payload));
      } catch (e) {
        console.error('[Kraken WS] Failed to send book subscription:', e);
      }
    } else {
      // Store subscription to send when connected
      const sendWhenReady = () => {
        if (this.isConnected) {
          try {
            this.send(JSON.stringify(payload));
          } catch (e) {
            console.error('[Kraken WS] Failed to send book subscription:', e);
          }
        } else {
          setTimeout(sendWhenReady, 100);
        }
      };
      setTimeout(sendWhenReady, 100);
    }

    return () => {
      this.orderBookHandlers[symbol] = (this.orderBookHandlers[symbol] || []).filter((cb) => cb !== callback);
      this.clearFeedStatsIfIdle('book', this.orderBookHandlers);
    };
  };

  private handleBookSnapshot = (msg: any) => {
    const symbol = v(msg, 'product_id');
    if (!symbol) return;

    const handlers = this.orderBookHandlers[symbol] || [];
    if (handlers.length === 0) {
      return;
    }

    this.recordMessageHandlerStats('book');

    // Log the raw data to debug quantity parsing
    const rawBids = v(msg, 'bids') || [];
    const rawAsks = v(msg, 'asks') || [];

    // Initialize or clear the cache for this symbol
    this.orderBookCache[symbol] = {
      bids: new Map(),
      asks: new Map(),
    };

    // Process bids (they come as objects with price and qty)
    for (const bid of rawBids) {
      const price = parseFloat(bid.price);
      const qty = parseFloat(bid.qty);
      if (qty > 0) {
        this.orderBookCache[symbol].bids.set(price, qty);
      }
    }

    // Process asks
    for (const ask of rawAsks) {
      const price = parseFloat(ask.price);
      const qty = parseFloat(ask.qty);
      if (qty > 0) {
        this.orderBookCache[symbol].asks.set(price, qty);
      }
    }

    // Emit the orderbook
    this.emitOrderBook(symbol);
  };

  private handleBookDelta = (msg: any) => {
    const symbol = v(msg, 'product_id');
    if (!symbol) return;

    const handlers = this.orderBookHandlers[symbol] || [];
    if (handlers.length === 0) {
      return;
    }

    this.recordMessageHandlerStats('book');

    // Make sure we have a cache for this symbol
    if (!this.orderBookCache[symbol]) {
      return;
    }

    const side = v(msg, 'side');
    const price = parseFloat(v(msg, 'price'));
    const qty = parseFloat(v(msg, 'qty'));

    // Update the appropriate side
    const bookSide = side === 'buy' ? this.orderBookCache[symbol].bids : this.orderBookCache[symbol].asks;

    if (qty === 0) {
      // Remove the price level
      bookSide.delete(price);
    } else {
      // Update the price level
      bookSide.set(price, qty);
    }

    // Emit the updated orderbook
    this.emitOrderBook(symbol);
  };

  private emitOrderBook = (symbol: string) => {
    const handlers = this.orderBookHandlers[symbol] || [];
    if (handlers.length === 0) return;

    const cache = this.orderBookCache[symbol];
    if (!cache) return;

    // Get the market to find contract size
    const market = this.parent.store.markets.find((m) => m.symbol === symbol);
    const contractSize = market?.contractSize || 1;

    // Convert maps to sorted arrays and multiply quantities by contract size
    const bids: OrderBook['bids'] = Array.from(cache.bids.entries())
      .sort((a, b) => b[0] - a[0]) // Sort descending by price
      .slice(0, 100) // Limit depth
      .map(([price, amount]) => ({
        price,
        amount: amount * contractSize, // Multiply by contract size
        total: 0,
      }));

    const asks: OrderBook['asks'] = Array.from(cache.asks.entries())
      .sort((a, b) => a[0] - b[0]) // Sort ascending by price
      .slice(0, 100) // Limit depth
      .map(([price, amount]) => ({
        price,
        amount: amount * contractSize, // Multiply by contract size
        total: 0,
      }));

    handlers.forEach((cb) => cb({ bids, asks }));
  };

  private mapTimeframe(tf: Timeframe): number {
    const table: Record<Timeframe, number> = {
      '1m': 1,
      '3m': 3,
      '5m': 5,
      '15m': 15,
      '30m': 30,
      '1h': 60,
      '2h': 120,
      '4h': 240,
      '6h': 360,
      '12h': 720,
      '1d': 1440,
      '1w': 10080,
    };
    return table[tf] ?? 1;
  }
}
