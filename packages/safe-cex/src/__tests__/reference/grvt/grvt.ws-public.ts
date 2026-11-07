import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, OHLCVOptions, OrderBook } from '../../../types';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { GrvtExchange } from './grvt.exchange';
import { GrvtCandlestickInterval, WSS_URLS } from '../../../exchanges/grvt/grvt.types';

/**
 * GRVT Public WebSocket - Market data streams
 *
 * Channels:
 * - candlestick: Candlestick data (OHLCV)
 * - orderbook_levels: Real-time orderbook snapshots
 * - trades: Recent trades
 * - ticker: Ticker updates
 * - funding: Funding rate updates
 *
 * WebSocket uses JSON-RPC 2.0 protocol
 */
export class GrvtPublicWebSocket extends BaseWebSocket<GrvtExchange> {
  private subscriptions: Map<number, any> = new Map();
  private requestId = 1;
  private topics: Record<string, number> = {};
  // Keep each market's ticker subscription stable across reconnects. The
  // subscription map is replayed by resubscribe(), so creating a replacement
  // here would send duplicate upstream topics on every reconnect.
  private tickerSubscriptions = new Map<string, number>();
  private tickerFields = new Map<string, { buy: number; sell: number; open: number }>();
  // The staggered batch callbacks outlive their originating socket unless we
  // retain them. Track both handles and a generation so a reconnect, manual
  // unsubscribe, or disposal cannot let stale work mutate subscription state.
  private tickerBatchTimers = new Set<ReturnType<typeof setTimeout>>();
  private tickerBatchGeneration = 0;

  constructor(parent: GrvtExchange) {
    super(parent);
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const env = this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION';
      const url = WSS_URLS[env];
      this.ws = this.createWebsocket(url);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.resubscribe();
      // subscribeToAllTickers only creates topics that are not already in the
      // replay map, including batches that were queued before a reconnect.
      this.subscribeToAllTickers();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);
    }
  };

  private onMarketsChanged = ({ added }: { added: Array<{ id: string; symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;
    if (!this.isConnected) return;

    for (const market of added) {
      try {
        this.ensureTickerSubscription(market.id);
      } catch (error) {
        this.parent.emitAndLogError(`Error subscribing to ticker for ${market.symbol}: ${error}`);
      }
    }
  };

  private resubscribe = () => {
    // Re-subscribe to all active subscriptions after reconnection
    for (const [_id, subscription] of this.subscriptions) {
      this.send(JSON.stringify(subscription));
    }
  };

  /**
   * Subscribe to ticker updates for all markets
   * Uses v1.ticker.d (delta) for efficiency - only changed fields are sent
   */
  subscribeToAllTickers = () => {
    if (this.isDisposed || !this.isConnected) {
      return;
    }

    this.cancelTickerBatches();
    const batchGeneration = this.tickerBatchGeneration;
    // Subscribe to ticker updates for all markets
    const markets = this.parent.store.markets;
    if (markets.length === 0) {
      return;
    }

    // Subscribe in batches to avoid overwhelming the server
    const batchSize = 20;
    for (let i = 0; i < markets.length; i += batchSize) {
      const batch = markets.slice(i, i + batchSize);

      const timeout = setTimeout(
        () => {
          this.tickerBatchTimers.delete(timeout);
          if (this.isDisposed || batchGeneration !== this.tickerBatchGeneration || !this.isConnected) {
            return;
          }
          for (const market of batch) {
            try {
              this.ensureTickerSubscription(market.id);
            } catch (error) {
              this.parent.emitAndLogError(`Error subscribing to ticker for ${market.symbol}: ${error}`);
            }
          }
        },
        (i / batchSize) * 100,
      ); // Stagger batches by 100ms
      this.tickerBatchTimers.add(timeout);
    }
  };

  private cancelTickerBatches = () => {
    this.tickerBatchGeneration += 1;
    for (const timer of this.tickerBatchTimers) {
      clearTimeout(timer);
    }
    this.tickerBatchTimers.clear();
  };

  private ensureTickerSubscription = (instrumentId: string) => {
    if (this.isDisposed || !this.isConnected) {
      return;
    }
    if (this.tickerSubscriptions.has(instrumentId)) {
      return this.tickerSubscriptions.get(instrumentId)!;
    }

    const subscriptionId = this.subscribeToTicker(instrumentId, 500, 'v1.ticker.d');
    this.tickerSubscriptions.set(instrumentId, subscriptionId);
    return subscriptionId;
  };

  /**
   * Map common timeframe format to GRVT candlestick interval enum
   */
  private mapIntervalToGrvt = (interval: string): string => {
    const mapping: Record<string, string> = {
      '1m': GrvtCandlestickInterval.CI_1_M,
      '3m': GrvtCandlestickInterval.CI_3_M,
      '5m': GrvtCandlestickInterval.CI_5_M,
      '15m': GrvtCandlestickInterval.CI_15_M,
      '30m': GrvtCandlestickInterval.CI_30_M,
      '1h': GrvtCandlestickInterval.CI_1_H,
      '2h': GrvtCandlestickInterval.CI_2_H,
      '4h': GrvtCandlestickInterval.CI_4_H,
      '6h': GrvtCandlestickInterval.CI_6_H,
      '8h': GrvtCandlestickInterval.CI_8_H,
      '12h': GrvtCandlestickInterval.CI_12_H,
      '1d': GrvtCandlestickInterval.CI_1_D,
      '3d': GrvtCandlestickInterval.CI_3_D,
      '5d': GrvtCandlestickInterval.CI_5_D,
      '1w': GrvtCandlestickInterval.CI_1_W,
      '2w': GrvtCandlestickInterval.CI_2_W,
      '3w': GrvtCandlestickInterval.CI_3_W,
      '4w': GrvtCandlestickInterval.CI_4_W,
    };
    return mapping[interval] || interval;
  };

  subscribeToKline = (instrumentId: string, interval: string, type: 'TRADE' | 'MARK' = 'TRADE') => {
    const id = this.requestId++;
    const grvtInterval = this.mapIntervalToGrvt(interval);
    const selector = `${instrumentId}@${grvtInterval}-${type}`;

    const subscription = {
      jsonrpc: '2.0',
      id,
      method: 'subscribe',
      params: {
        stream: 'v1.candle',
        selectors: [selector],
      },
    };

    this.subscriptions.set(id, subscription);
    this.send(JSON.stringify(subscription));
    return id;
  };

  unsubscribeFromKline = (subscriptionId: number) => {
    if (this.subscriptions.has(subscriptionId)) {
      const unsubscribe = {
        jsonrpc: '2.0',
        id: this.requestId++,
        method: 'unsubscribe',
        params: {
          subscription_id: subscriptionId,
        },
      };

      this.send(JSON.stringify(unsubscribe));
      this.subscriptions.delete(subscriptionId);
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        // Handle subscription confirmations
        if (json.result && json.id) {
          return;
        }

        // Handle error responses
        if (json.error) {
          // Ignore "Stream not supported on this gateway" errors (code 1100)
          // This happens when trying to subscribe to streams not available on market data gateway
          if (json.error.code === 1100) {
            return;
          }
          this.parent.emitAndLogError(
            `WebSocket error (code=${typeof json.error.code === 'number' ? json.error.code : 'unknown'})`,
          );
          return;
        }

        // Handle stream data updates (new format)
        // Check for stream field (full format: "stream", lite format: "s")
        const stream = json.s || json.stream;
        if (stream) {
          if (stream === 'v1.book.s') {
            this.handleOrderbookUpdate(json);
          } else if (stream === 'v1.candle') {
            this.handleKlineUpdate(json);
          } else if (stream === 'v1.ticker.s' || stream === 'v1.ticker.d') {
            this.handleTickerUpdate(json);
          }
        }
      } catch (error) {
        this.parent.emitAndLogError(`Error parsing WebSocket message: ${error}`);
      }
    }
  };

  private handleKlineUpdate = (message: any) => {
    try {
      // Extract feed data from message
      // Lite format: { s: 'v1.candle', s1: 'BTC_USDT_Perp@CI_1_M-TRADE', f: {...} }
      // Full format: { stream: 'v1.candle', selector: 'BTC_USDT_Perp@CI_1_M-TRADE', feed: {...} }
      const feed = message.f || message.feed;
      if (!feed) return;

      // Get instrument from feed data
      const instrument = feed.i || feed.instrument;
      if (!instrument) return;

      // The selector contains the full subscription key: instrument@interval-type
      // We need to extract just the interval part for the topic key
      const selector = message.s1 || message.selector;
      if (!selector) return;

      // Extract interval from selector (format: BTC_USDT_Perp@CI_1_M-TRADE)
      const parts = selector.split('@');
      if (parts.length !== 2) return;

      const intervalPart = parts[1]; // "CI_1_M-TRADE"
      const intervalMatch = intervalPart.split('-')[0]; // "CI_1_M"

      const topic = `kline.${intervalMatch}.${instrument}`;
      const handlers = this.messageHandlers[topic];

      if (handlers && handlers.length > 0) {
        this.recordMessageHandlerStats(topic);
        handlers.forEach((handler) => handler(feed));
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling kline update: ${error}`);
    }
  };

  subscribeToOrderbook = (instrumentId: string, refreshMs: number = 500, depth: number = 50) => {
    const id = this.requestId++;
    const selector = `${instrumentId}@${refreshMs}-${depth}`;

    const subscription = {
      jsonrpc: '2.0',
      id,
      method: 'subscribe',
      params: {
        stream: 'v1.book.s',
        selectors: [selector],
      },
    };

    this.subscriptions.set(id, subscription);
    this.send(JSON.stringify(subscription));
    return id;
  };

  unsubscribeFromOrderbook = (subscriptionId: number) => {
    if (this.subscriptions.has(subscriptionId)) {
      const unsubscribe = {
        jsonrpc: '2.0',
        id: this.requestId++,
        method: 'unsubscribe',
        params: {
          subscription_id: subscriptionId,
        },
      };

      this.send(JSON.stringify(unsubscribe));
      this.subscriptions.delete(subscriptionId);
    }
  };

  private handleOrderbookUpdate = (message: any) => {
    try {
      // Extract feed data from message
      // Lite format: { s: 'v1.book.s', s1: 'BTC_USDT_Perp', sn: '...', f: {...} }
      // Full format: { stream: 'v1.book.s', selector: 'BTC_USDT_Perp', sequence_number: '...', feed: {...} }
      const feed = message.f || message.feed;
      if (!feed) return;

      // Get instrument from feed data
      const instrument = feed.i || feed.instrument;
      if (!instrument) return;

      const topic = `orderbook.${instrument}`;
      const handlers = this.messageHandlers[topic];

      if (handlers && handlers.length > 0) {
        this.recordMessageHandlerStats(topic);
        handlers.forEach((handler) => handler(feed));
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling orderbook update: ${error}`);
    }
  };

  subscribeToTicker = (
    instrumentId: string,
    refreshMs: number = 500,
    stream: 'v1.ticker.s' | 'v1.ticker.d' = 'v1.ticker.s',
  ) => {
    const id = this.requestId++;
    const selector = `${instrumentId}@${refreshMs}`;

    const subscription = {
      jsonrpc: '2.0',
      id,
      method: 'subscribe',
      params: {
        stream,
        selectors: [selector],
      },
    };

    this.subscriptions.set(id, subscription);
    this.send(JSON.stringify(subscription));
    return id;
  };

  unsubscribeFromTicker = (subscriptionId: number) => {
    if (this.subscriptions.has(subscriptionId)) {
      // A pending batch may otherwise recreate the just-removed logical topic.
      this.cancelTickerBatches();
      const unsubscribe = {
        jsonrpc: '2.0',
        id: this.requestId++,
        method: 'unsubscribe',
        params: {
          subscription_id: subscriptionId,
        },
      };

      this.send(JSON.stringify(unsubscribe));
      this.subscriptions.delete(subscriptionId);
      for (const [instrumentId, tickerSubscriptionId] of this.tickerSubscriptions) {
        if (tickerSubscriptionId === subscriptionId) {
          this.tickerSubscriptions.delete(instrumentId);
          break;
        }
      }
    }
  };

  private handleTickerUpdate = (message: any) => {
    try {
      // Extract feed data from message
      // Lite format: { s: 'v1.ticker.s', s1: 'BTC_USDT_Perp', sn: '...', f: {...} }
      // Full format: { stream: 'v1.ticker.s', selector: 'BTC_USDT_Perp', sequence_number: '...', feed: {...} }
      const feed = message.f || message.feed;
      if (!feed) return;

      // Get instrument from feed data
      const instrument = feed.i || feed.instrument;
      if (!instrument) return;

      // Find market by instrument ID
      // Use O(1) hashmap lookup instead of O(n) find
      const market = this.parent.store.marketsById[instrument];
      if (!market) return;

      const currentTicker = this.parent.store.tickersById[market.id];
      const fields = this.tickerFields.get(market.id) ?? { buy: 0, sell: 0, open: 0 };
      this.tickerFields.set(market.id, fields);
      const number = (value: string | number) => (value === '' ? 0 : Number(value));
      // Build changes object - only include fields that are present in the feed
      // This is important for delta streams (v1.ticker.d) which only send changed fields
      const changes: Record<string, any> = {
        symbol: market.symbol,
        info: feed,
      };

      // Parse ticker data (handle both lite and full formats)
      // Only add fields that are actually present in the message
      const markPriceRaw = feed.mp ?? feed.mark_price;
      if (markPriceRaw !== undefined) {
        changes.mark = number(markPriceRaw);
      }

      const indexPriceRaw = feed.ip ?? feed.index_price;
      if (indexPriceRaw !== undefined) {
        changes.index = number(indexPriceRaw);
      }

      const lastPriceRaw = feed.lp ?? feed.last_price;
      if (lastPriceRaw !== undefined) {
        changes.last = number(lastPriceRaw);
      }

      const bestBidPriceRaw = feed.bb ?? feed.best_bid_price;
      if (bestBidPriceRaw !== undefined) {
        changes.bid = number(bestBidPriceRaw);
      }

      const bestAskPriceRaw = feed.ba ?? feed.best_ask_price;
      if (bestAskPriceRaw !== undefined) {
        changes.ask = number(bestAskPriceRaw);
      }

      const fundingRateRaw = feed.fr2 ?? feed.funding_rate;
      if (fundingRateRaw !== undefined) {
        // GRVT sends funding rate as a percentage (e.g. 0.01 = 0.01%).
        // Tealstreet stores it as a decimal fraction (0.0001 = 0.01%).
        changes.fundingRate = number(fundingRateRaw) / 100;
      }

      const openInterestRaw = feed.oi ?? feed.open_interest;
      if (openInterestRaw !== undefined) {
        changes.openInterest = number(openInterestRaw);
      }

      // Volume fields: bv/buy_volume_24h_b, sv/sell_volume_24h_b
      const buyVolumeRaw = feed.bv ?? feed.buy_volume_24h_b;
      const sellVolumeRaw = feed.sv ?? feed.sell_volume_24h_b;
      if (buyVolumeRaw !== undefined) fields.buy = number(buyVolumeRaw);
      if (sellVolumeRaw !== undefined) fields.sell = number(sellVolumeRaw);
      changes.volume = fields.buy + fields.sell;
      const last = changes.last ?? currentTicker?.last ?? 0;
      changes.quoteVolume = changes.volume * last;

      const openPriceRaw = feed.op ?? feed.open_price;
      if (openPriceRaw !== undefined) fields.open = number(openPriceRaw);
      if (fields.open > 0) changes.percentage = ((last - fields.open) / fields.open) * 100;

      // Parse next funding time (nanoseconds to milliseconds)
      const nextFundingTimeRaw = feed.nf ?? feed.next_funding_time;
      if (nextFundingTimeRaw !== undefined) {
        const fundingTimeMs =
          number(nextFundingTimeRaw) / 1000000;
        changes.fundingTime = fundingTimeMs / 1000; // Convert to seconds
      }

      // Fallback: if we have mark price but no last price, use mark as last
      // This ensures ticker.last is populated even when delta stream doesn't send last_price
      if (changes.last === undefined && changes.mark !== undefined && changes.mark > 0) {
        // Check if current ticker's last is 0 or missing
        // Use O(1) hashmap lookup instead of O(n) find
        if (!currentTicker?.last || currentTicker.last === 0) {
          changes.last = changes.mark;
        }
      }

      if (changes.last !== undefined) {
        changes.quoteVolume = changes.volume * changes.last;
        if (fields.open > 0) changes.percentage = ((changes.last - fields.open) / fields.open) * 100;
      }

      // Update ticker in store
      this.parent.store.updateTicker({ id: market.id }, changes);

      // Keep margin upnl in sync with live ticker moves for open positions.
      const hasOpenPositionForSymbol = this.parent.store.positions.some(
        (position) => position.symbol === market.symbol && Math.abs(position.contracts) > 0,
      );
      if (hasOpenPositionForSymbol) {
        this.parent.syncBalanceUpnlFromPositions();
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling ticker update: ${error}`);
    }
  };

  /**
   * Listen to OHLCV candles for a specific market and interval
   */
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) {
      this.parent.emitAndLogError(`Market with symbol ${opts.symbol} not found`);
      return () => {};
    }

    const instrumentId = market.id;
    const interval = opts.interval;
    const grvtInterval = this.mapIntervalToGrvt(interval);
    const topic = `kline.${grvtInterval}.${instrumentId}`;
    const handler = topic;

    let subscriptionId: number | null = null;

    const parser = (feed: any) => {
      // Feed format (lite/full):
      // { i/instrument, ot/open_time, ct/close_time, o/open, h/high, l/low, c/close, vb/volume_b }
      const feedInstrument = feed.i || feed.instrument;
      if (feedInstrument === instrumentId) {
        // Timestamps are in nanoseconds, convert to seconds
        const openTime = feed.ot || feed.open_time;
        const timestampSeconds = typeof openTime === 'string' ? parseInt(openTime) / 1000000000 : openTime / 1000;

        const closePrice = parseFloat(feed.c || feed.close || '0');

        callback({
          timestamp: timestampSeconds,
          open: parseFloat(feed.o || feed.open || '0'),
          high: parseFloat(feed.h || feed.high || '0'),
          low: parseFloat(feed.l || feed.low || '0'),
          close: closePrice,
          volume: parseFloat(feed.vb || feed.volume_b || feed.volume || '0'),
        });

        // Update ticker's last price from candle close
        // This ensures the ticker is updated even if ticker WS stream doesn't send last_price
        if (closePrice > 0) {
          this.parent.store.updateTicker({ id: market.id }, { last: closePrice });
        }
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        subscriptionId = this.subscribeToKline(instrumentId, interval);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        if (subscriptionId !== null) {
          this.unsubscribeFromKline(subscriptionId);
        }
      }
    };
  };

  /**
   * Listen to orderbook updates for a specific market
   */
  listenOrderBook = (symbol: string, callback: (orderbook: OrderBook) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === symbol);
    if (!market) {
      this.parent.emitAndLogError(`Market with symbol ${symbol} not found`);
      return () => {};
    }

    const instrumentId = market.id;
    const topic = `orderbook.${instrumentId}`;
    const handler = topic;

    let subscriptionId: number | null = null;

    const parser = (data: any) => {
      // Handle both lite format (i) and full format (instrument)
      const dataInstrument = data.i || data.instrument;
      if (dataInstrument === instrumentId) {
        // Parse bids and asks from GRVT format to safe-cex format
        // Handle both lite format (b for bids, a for asks) and full format (bids, asks)
        const rawBids = data.b || data.bids || [];
        const rawAsks = data.a || data.asks || [];

        const bids = rawBids.map((bid: any, index: number, arr: any[]) => {
          // Handle both lite format (p, s) and full format (price, size)
          const price = parseFloat(bid.p || bid.price);
          const amount = parseFloat(bid.s || bid.size);
          // Calculate cumulative total (sum of all amounts up to this level)
          let total = amount;
          for (let i = 0; i < index; i++) {
            total += parseFloat(arr[i].s || arr[i].size);
          }
          return { price, amount, total };
        });

        const asks = rawAsks.map((ask: any, index: number, arr: any[]) => {
          // Handle both lite format (p, s) and full format (price, size)
          const price = parseFloat(ask.p || ask.price);
          const amount = parseFloat(ask.s || ask.size);
          // Calculate cumulative total (sum of all amounts up to this level)
          let total = amount;
          for (let i = 0; i < index; i++) {
            total += parseFloat(arr[i].s || arr[i].size);
          }
          return { price, amount, total };
        });

        callback({ bids, asks });
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        subscriptionId = this.subscribeToOrderbook(instrumentId);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        if (subscriptionId !== null) {
          this.unsubscribeFromOrderbook(subscriptionId);
        }
      }
    };
  };

  dispose = () => {
    this.cancelTickerBatches();
    this.subscriptions.clear();
    this.tickerSubscriptions.clear();
    this.parent.emitter.off('markets:changed', this.onMarketsChanged);
    super.dispose();
  };
}
