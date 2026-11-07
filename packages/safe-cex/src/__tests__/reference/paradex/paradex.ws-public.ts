import throttle from 'lodash/throttle';

import { Candle, LogSeverity, OHLCVOptions, OrderBook, Ticker, timeframeToSeconds } from '../../../types';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { ParadexExchange } from './paradex.exchange';
import { BASE_WS_URL } from './paradex.types';
import { decodeParadexMessage } from './paradex.sbe';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

const getOrderBookChannel = (symbol: string) => `order_book.${symbol}.deltas`;
const OHLCV_POLL_INTERVAL_MS = 3_000;
const OHLCV_TRADE_STALE_MS = 20_000;
const OHLCV_RECOVERY_COOLDOWN_MS = 30_000;
const SUBSCRIPTION_RETRY_DELAY_MS = 1_000;
const MAX_SUBSCRIPTION_RETRIES = 2;

type PendingSubscribeRequest = {
  channel: string;
  attempt: number;
};

type DepthLevel = { side: 'BUY' | 'SELL'; price: number; size: number };
type DepthState = {
  sequence?: bigint;
  initialized: boolean;
  recovering: boolean;
  bids: Map<number, number>;
  asks: Map<number, number>;
  emit: ReturnType<typeof throttle>;
};

export class ParadexPublicWebSocket extends BaseWebSocket<ParadexExchange> {
  connectUrl = '';
  private orderbookCallbacks: Map<string, Set<(orderbook: OrderBook) => void>> = new Map();
  private orderbooks = new Map<string, DepthState>();
  private tradeSubscriptionCounts: Map<string, number> = new Map();
  private desiredTradeSymbols = new Set<string>();
  private sentTradeSymbols = new Set<string>();
  private pendingSubscribeRequests = new Map<number, PendingSubscribeRequest>();
  private subscriptionRetryTimeouts = new Set<NodeJS.Timeout>();
  private messageId = 0;

  // OHLCV polling state + trades integration
  private ohlcvPollingTimeouts = new Map<string, NodeJS.Timeout>();
  private ohlcvPollingActive = new Map<string, boolean>();
  private ohlcvSubscriptionVersions = new Map<string, number>();
  private lastCandleCache = new Map<string, Candle>();
  private ohlcvCallbacks = new Map<string, Set<(candle: Candle) => void>>(); // Store callbacks for trade updates
  private ohlcvIntervals = new Map<string, number>(); // Store interval in ms for each subscription
  private ohlcvLastTradeAt = new Map<string, number>();
  private ohlcvRevisions = new Map<string, number>();
  private ohlcvLastRecoveryAt = new Map<string, number>();

  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePongEvent()],
    pong: [() => this.handlePongEvent()],
    // bbo provides per-symbol best bid/ask tick updates at near-real-time cadence.
    // Without it, watchlist/positions PnL becomes laggy because markets_summary
    // alone does not push frequently enough for live UX.
    bbo: [(data: Data) => this.handleBBOUpdate(data)],
    // markets_summary kept for slow-changing fields (volume, openInterest, fundingRate)
    // that bbo doesn't carry. Every event updates its market; no global throttle.
    markets_summary: [(data: Data) => this.handleMarketSummaryUpdate(data)],
    order_book: [(data: Data) => this.handleOrderbookUpdate(data)],
    trades: [(data: Data) => this.handleTradesUpdate(data)],
  };

  constructor(parent: ParadexExchange) {
    super(parent);
    this.connectUrl = BASE_WS_URL.public[parent.options.testnet ? 'testnet' : 'livenet'];
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(this.connectUrl);
      this.ws.binaryType = 'arraybuffer';
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.parent.log('Paradex public WebSocket connected', LogSeverity.Info);
      this.sentTradeSymbols.clear();
      this.pendingSubscribeRequests.clear();
      this.clearOrderbooks();

      // Verify websocket is still connected before subscribing
      if (!this.isConnected) {
        this.parent.log('WebSocket disconnected in onOpen, closing connection', LogSeverity.Warning);
        this.ws?.close();
        return;
      }

      this.subscribeToChannels();
    }
  };

  onMessage = (event: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      let data: Data | null;
      try {
        data = decodeParadexMessage(event.data);
      } catch (error) {
        this.parent.log(`Invalid Paradex websocket frame: ${error}`, LogSeverity.Warning);
        return;
      }
      if (!data) return;

      // Handle JSON-RPC 2.0 format
      if (data.jsonrpc === '2.0') {
        // Handle subscription updates
        if (data.method === 'subscription' && data.params) {
          const channel = data.params.channel;
          // Extract channel name (e.g., "order_book.BTC-USD-PERP.snapshot@15@100ms" -> "order_book")
          const channelParts = channel?.split('.') || [];
          const channelType = channelParts[0];
          const symbol = channelParts[1]; // Symbol if present (e.g., "BTC-USD-PERP")
          const handlers = this.messageHandlers[channelType];

          if (handlers) {
            // Record message stats to track WebSocket health
            // Only orderbook needs per-symbol tracking (users subscribe to specific orderbooks)
            // markets_summary and trades are tracked generically (we just need *some* updates)
            if (symbol && channelType === 'order_book') {
              this.recordMessageHandlerStats(`${channelType}.${symbol}`);
            } else {
              this.recordMessageHandlerStats(channelType);
            }
            handlers.forEach((handler) => handler(data.params));
          } else if (channelType) {
            this.parent.log(`No handler for channel type: ${channelType} (channel: ${channel})`, LogSeverity.Warning);
          }
        }
        // Handle responses (success/error)
        else if (data.id !== undefined) {
          const pendingSubscribe = this.pendingSubscribeRequests.get(data.id);
          if (pendingSubscribe) {
            this.pendingSubscribeRequests.delete(data.id);
            if (data.error || data.result?.channel !== pendingSubscribe.channel) {
              this.retrySubscribeChannel(
                pendingSubscribe.channel, pendingSubscribe.attempt,
                data.error?.message || 'missing or mismatched subscription acknowledgement',
              );
            }
          }
          if (data.error) {
            this.parent.log(`WebSocket error for request ${data.id}: ${data.error.message}`, LogSeverity.Error);
          }
        }
      }
      // Legacy format (ping/pong)
      else {
        const topic = data.type || data.channel;
        const handlers = this.messageHandlers[topic];
        if (handlers) {
          // Record message stats to track WebSocket health
          this.recordMessageHandlerStats(topic);
          handlers.forEach((handler) => handler(data));
        }
      }
    }
  };

  // The upstream uses RFC6455 control pings, answered by the socket library.
  protected treatAnyMessageAsPong = true;
  ping = () => undefined;
  getWatchdogStaleThresholdMs = () => 90_000;

  dispose = () => {
    this.subscriptionRetryTimeouts.forEach((timeoutId) => clearTimeout(timeoutId));
    this.subscriptionRetryTimeouts.clear();
    this.ohlcvPollingTimeouts.forEach((timeoutId) => clearTimeout(timeoutId));
    this.ohlcvPollingTimeouts.clear();
    this.clearOrderbooks();
    super.dispose();
  };

  private sendJsonRpcSubscription = (method: 'subscribe' | 'unsubscribe', channel: string, attempt = 0) => {
    if (!this.isConnected) {
      return;
    }

    const id = this.messageId++;
    if (method === 'subscribe') {
      this.pendingSubscribeRequests.set(id, { channel, attempt });
    }

    this.send(
      JSON.stringify({
        id,
        jsonrpc: '2.0',
        method,
        params: { channel },
      }),
    );
  };

  private retrySubscribeChannel = (channel: string, attempt: number, reason?: string) => {
    if (attempt >= MAX_SUBSCRIPTION_RETRIES || !this.hasActiveSubscriptionInterest(channel)) {
      return;
    }

    if (channel.startsWith('trades.')) {
      const symbol = channel.split('.')[1];
      if (symbol) {
        this.sentTradeSymbols.delete(symbol);
      }
    }

    this.parent.log(
      `Retrying Paradex subscription ${channel} after error${reason ? `: ${reason}` : ''}`,
      LogSeverity.Warning,
    );

    const timeoutId = setTimeout(() => {
      this.subscriptionRetryTimeouts.delete(timeoutId);
      if (this.isDisposed || !this.hasActiveSubscriptionInterest(channel)) {
        return;
      }
      this.sendJsonRpcSubscription('subscribe', channel, attempt + 1);
    }, SUBSCRIPTION_RETRY_DELAY_MS * (attempt + 1));

    this.subscriptionRetryTimeouts.add(timeoutId);
  };

  private hasActiveSubscriptionInterest = (channel: string): boolean => {
    if (channel === 'markets_summary') {
      return this.parent.store.markets.length > 0;
    }

    if (channel.startsWith('trades.')) {
      const symbol = channel.split('.')[1];
      return Boolean(
        symbol && (this.desiredTradeSymbols.has(symbol) || this.parent.store.markets.some((m) => m.symbol === symbol)),
      );
    }

    if (channel.startsWith('bbo.')) {
      const symbol = channel.split('.')[1];
      return Boolean(symbol && this.parent.store.markets.some((m) => m.symbol === symbol));
    }

    if (channel.startsWith('order_book.')) {
      const symbol = channel.split('.')[1];
      return Boolean(symbol && this.orderbookCallbacks.get(symbol)?.size);
    }

    return false;
  };

  private sendTradeSubscribe = (symbol: string, force = false, attempt = 0) => {
    if (!symbol) {
      return;
    }
    this.desiredTradeSymbols.add(symbol);
    if (!force && this.sentTradeSymbols.has(symbol)) {
      return;
    }
    this.sentTradeSymbols.add(symbol);
    this.sendJsonRpcSubscription('subscribe', `trades.${symbol}`, attempt);
  };

  private handleBBOUpdate = (data: any) => {
      try {
        const bboData = data?.data;
        if (!bboData) {
          return;
        }

        const channel = String(data?.channel || '');
        const channelSymbol = channel.split('.')[1];
        const symbol = String(bboData.market || channelSymbol || '');
        if (!symbol) {
          return;
        }

        // BBO format: { market, bid_price, bid_size, ask_price, ask_size, mark_price, index_price }
        // last_price is intentionally NOT updated from bbo — that comes from the trades feed.
        const bid = parseFloat(bboData.bid_price || '0');
        const ask = parseFloat(bboData.ask_price || '0');
        const index = parseFloat(bboData.index_price || '0');

        const update: Partial<Ticker> = {
          symbol,
          ...(Number.isFinite(bid) && bid > 0 ? { bid } : {}),
          ...(Number.isFinite(ask) && ask > 0 ? { ask } : {}),
          ...(Number.isFinite(index) && index > 0 ? { index } : {}),
        };

        this.parent.store.updateTicker({ id: symbol }, update);
      } catch (err) {
        this.parent.log(`Error handling BBO update: ${err}`, LogSeverity.Error);
      }
  };

  private handleMarketSummaryUpdate = (data: any) => {
      try {
        const rawData = Array.isArray(data?.data) ? data.data : [data?.data || data];
        for (const summary of rawData) {
          if (!summary || typeof summary !== 'object') {
            continue;
          }

          const symbol = String(summary.symbol || summary.market || '');
          if (!symbol) {
            continue;
          }

          const bid = parseFloat(summary.bid || '0');
          const ask = parseFloat(summary.ask || '0');
          const last = parseFloat(summary.last_traded_price || summary.last_price || summary.last || '0');
          const mark = parseFloat(summary.mark_price || '0');
          const index = parseFloat(summary.oracle_price || summary.index_price || summary.underlying_price || '0');
          const percentageRaw = parseFloat(summary.price_change_rate_24h || '0');
          const volume = parseFloat(summary.volume_24h || summary.volume || '0');
          const openInterest = parseFloat(summary.open_interest || '0');
          const fundingRate = parseFloat(summary.funding_rate || '0');

          this.parent.store.updateTicker(
            { id: symbol },
            {
              symbol,
              ...(Number.isFinite(bid) && bid > 0 ? { bid } : {}),
              ...(Number.isFinite(ask) && ask > 0 ? { ask } : {}),
              ...(Number.isFinite(last) && last > 0 ? { last } : {}),
              ...(Number.isFinite(mark) && mark > 0 ? { mark } : {}),
              ...(Number.isFinite(index) && index > 0 ? { index } : {}),
              ...(Number.isFinite(percentageRaw) ? { percentage: percentageRaw * 100 } : {}),
              ...(Number.isFinite(volume) ? { volume } : {}),
              ...(Number.isFinite(openInterest) ? { openInterest } : {}),
              ...(Number.isFinite(fundingRate) ? { fundingRate } : {}),
            },
          );
        }
      } catch (err) {
        this.parent.log(`Error handling markets_summary update: ${err}`, LogSeverity.Error);
      }
  };

  private clearOrderbooks = () => {
    this.orderbooks.forEach((state) => state.emit.cancel());
    this.orderbooks.clear();
  };

  private depthState = (symbol: string): DepthState => {
    let state = this.orderbooks.get(symbol);
    if (!state) {
      state = {
        initialized: false,
        recovering: false,
        bids: new Map(),
        asks: new Map(),
        emit: throttle(() => this.emitOrderbook(symbol), 100, { leading: true, trailing: true }),
      };
      this.orderbooks.set(symbol, state);
    }
    return state;
  };

  private emitOrderbook = (symbol: string) => {
    const state = this.orderbooks.get(symbol);
    const callbacks = this.orderbookCallbacks.get(symbol);
    if (!state?.initialized || !callbacks?.size || this.isDisposed) return;
    const levels = (side: Map<number, number>, descending: boolean) => [...side]
      .sort(([a], [b]) => descending ? b - a : a - b)
      .map(([price, amount]) => ({ price, amount, total: price * amount }));
    const orderbook = { bids: levels(state.bids, true), asks: levels(state.asks, false) };
    if (orderbook.bids.length && orderbook.asks.length) {
      this.parent.store.updateTicker({ id: symbol }, {
        bid: orderbook.bids[0].price,
        ask: orderbook.asks[0].price,
      });
    }
    callbacks.forEach((callback) => callback(orderbook));
  };

  private recoverOrderbook = (symbol: string, state: DepthState) => {
    if (state.recovering) return;
    state.initialized = false;
    state.recovering = true;
    state.emit.cancel();
    if (this.isConnected) {
      const channel = getOrderBookChannel(symbol);
      this.sendJsonRpcSubscription('unsubscribe', channel);
      this.sendJsonRpcSubscription('subscribe', channel);
    }
  };

  private handleOrderbookUpdate = (data: Data) => {
    const rows = Array.isArray(data?.data) ? data.data : [data?.data];
    for (const row of rows) {
      const channelSymbol = String(data?.channel || '').split('.')[1];
      const symbol = String(row?.market || channelSymbol || '');
      if (!symbol || symbol !== channelSymbol || !this.orderbookCallbacks.get(symbol)?.size) continue;
      const state = this.depthState(symbol);
      const sequenceText = String(row?.seq_no ?? '');
      const sequenceValid = (typeof row?.seq_no === 'string' || Number.isSafeInteger(row?.seq_no)) && /^\d+$/.test(sequenceText);
      const decode = (value: unknown): DepthLevel[] | undefined => {
        if (!Array.isArray(value)) return;
        const levels: DepthLevel[] = [];
        for (const level of value) {
          const price = Number(level?.price);
          const size = Number(level?.size);
          if (
            !['BUY', 'SELL'].includes(level?.side) ||
            !['string', 'number'].includes(typeof level?.size) || String(level.size).trim() === '' ||
            !Number.isFinite(price) || price <= 0 || !Number.isFinite(size) || size < 0
          ) return;
          levels.push({ side: level.side, price, size });
        }
        return levels;
      };
      const inserts = decode(row?.inserts);
      const updates = decode(row?.updates);
      const deletes = decode(row?.deletes);
      if (!sequenceValid || !['s', 'd'].includes(row?.update_type) || !inserts || !updates || !deletes) {
        this.recoverOrderbook(symbol, state);
        continue;
      }
      const sequence = BigInt(sequenceText);
      if (state.sequence !== undefined && (
        sequence < state.sequence || (sequence === state.sequence && (row.update_type !== 's' || state.initialized))
      )) continue;
      if (row.update_type === 'd' && (!state.initialized || sequence !== state.sequence! + 1n)) {
        this.recoverOrderbook(symbol, state);
        continue;
      }
      if (row.update_type === 's') {
        state.bids.clear();
        state.asks.clear();
      }
      for (const level of [...inserts, ...updates]) {
        const side = level.side === 'BUY' ? state.bids : state.asks;
        if (level.size === 0) side.delete(level.price);
        else side.set(level.price, level.size);
      }
      for (const level of deletes) (level.side === 'BUY' ? state.bids : state.asks).delete(level.price);
      state.sequence = sequence;
      state.initialized = true;
      state.recovering = false;
      state.emit();
    }
  };

  private handleTradesUpdate = (data: any) => {
    try {
      // Debug: log when trades handler is called (commented out to reduce log spam)
      // this.parent.log(`Trades handler called with data: ${JSON.stringify(data)}`, LogSeverity.Debug);

      const rawTrades = Array.isArray(data?.data) ? data.data : [data?.data || data];
      const channel = String(data?.channel || '');
      const channelSymbol = channel.split('.')[1];

      if (rawTrades.length === 0) {
        this.parent.log(`Trades data is undefined. Full data: ${JSON.stringify(data)}`, LogSeverity.Warning);
        return;
      }

      for (const rawTrade of rawTrades) {
        const tradesData = rawTrade && typeof rawTrade === 'object' ? rawTrade : {};
        if (Object.keys(tradesData).length === 0) {
          continue;
        }

        const symbol = String(tradesData.market || channelSymbol || '');
        if (!symbol) {
          this.parent.log(`Trade missing market symbol. Data: ${JSON.stringify(tradesData)}`, LogSeverity.Warning);
          continue;
        }

        // Update last price from trade
        const tradePrice = parseFloat(tradesData.price || '0');
        const tradeSize = parseFloat(tradesData.size || '0');
        const createdAt = Number(tradesData.created_at ?? tradesData.timestamp);
        const tradeTimestamp = Number.isFinite(createdAt) && createdAt > 0 ? createdAt : Date.now();

        if (Number.isFinite(tradePrice) && tradePrice > 0) {
          // this.parent.log(
          //   `✅ Updating last price from trade: ${symbol} = ${tradePrice} (size=${tradesData.size || 'N/A'})`,
          //   LogSeverity.Info,
          // );

          this.parent.store.updateTicker(
            { id: symbol },
            {
              symbol,
              last: tradePrice,
              mark: tradePrice,
            },
          );

          // Update OHLCV candles for all active subscriptions for this symbol
          this.updateOHLCVFromTrade(symbol, tradePrice, Number.isFinite(tradeSize) ? tradeSize : 0, tradeTimestamp);

          // console.log(symbol, tradePrice);
        } else {
          this.parent.log(`Trade price is 0 or invalid: ${tradesData.price}`, LogSeverity.Warning);
        }
      }
    } catch (err) {
      this.parent.log(`Error handling trades update: ${err}`, LogSeverity.Error);
    }
  };

  private updateOHLCVFromTrade(symbol: string, price: number, size: number, timestamp: number) {
    // Check all active OHLCV subscriptions for this symbol
    for (const [pollKey, callbacks] of this.ohlcvCallbacks.entries()) {
      // Extract symbol from pollKey (format: "SYMBOL:interval")
      const [subscriptionSymbol] = pollKey.split(':');

      if (subscriptionSymbol !== symbol) {
        continue;
      }

      // Get the interval in milliseconds for this subscription
      const intervalMs = this.ohlcvIntervals.get(pollKey);
      if (!intervalMs) {
        continue;
      }
      // Get or create the current candle
      let currentCandle = this.lastCandleCache.get(pollKey);

      // Calculate the candle's timestamp (start of the interval period)
      const candleTimestamp = Math.floor(timestamp / intervalMs) * intervalMs;
      if (currentCandle && currentCandle.timestamp > candleTimestamp) continue;
      this.ohlcvLastTradeAt.set(pollKey, Date.now());
      this.ohlcvRevisions.set(pollKey, (this.ohlcvRevisions.get(pollKey) || 0) + 1);

      // If we don't have a candle or it's a new candle period, initialize it
      if (!currentCandle || currentCandle.timestamp !== candleTimestamp) {
        currentCandle = {
          timestamp: candleTimestamp,
          open: price,
          high: price,
          low: price,
          close: price,
          volume: size,
        };
      } else {
        // Update existing candle
        currentCandle = {
          ...currentCandle,
          high: Math.max(currentCandle.high, price),
          low: Math.min(currentCandle.low, price),
          close: price,
          volume: currentCandle.volume + size,
        };
      }

      // Update cache and emit if changed
      const previousCandle = this.lastCandleCache.get(pollKey);
      if (
        !previousCandle ||
        previousCandle.timestamp !== currentCandle.timestamp ||
        previousCandle.close !== currentCandle.close ||
        previousCandle.high !== currentCandle.high ||
        previousCandle.low !== currentCandle.low ||
        previousCandle.volume !== currentCandle.volume
      ) {
        this.lastCandleCache.set(pollKey, currentCandle);
        callbacks.forEach((callback) => callback(currentCandle));
      }
    }
  }

  private maybeRecoverOhlcvTradeStream = (opts: OHLCVOptions, pollKey: string) => {
    const now = Date.now();
    const lastTradeAt = this.ohlcvLastTradeAt.get(pollKey) || 0;
    if (lastTradeAt > 0 && now - lastTradeAt <= OHLCV_TRADE_STALE_MS) {
      return;
    }

    const lastRecoveryAt = this.ohlcvLastRecoveryAt.get(pollKey) || 0;
    if (now - lastRecoveryAt < OHLCV_RECOVERY_COOLDOWN_MS) {
      return;
    }

    this.ohlcvLastRecoveryAt.set(pollKey, now);
    this.parent.log(
      `No Paradex trade updates for ${opts.symbol} ${opts.interval}; re-subscribing trade channel`,
      LogSeverity.Warning,
    );
    this.sendTradeSubscribe(opts.symbol, true);
  };

  private subscribeToChannels = () => {
    const markets = this.parent.store.markets;

    // Per-symbol bbo + trades broadcast subscriptions: required for live bid/ask/last
    // updates on the watchlist and positions table. markets_summary alone is too slow
    // for tick-rate UX. Orderbook stays on-demand to avoid subscribing to 100+ books.
    markets.forEach((market) => {
      this.sendJsonRpcSubscription('subscribe', `bbo.${market.symbol}`);
    });

    markets.forEach((market) => {
      this.sendTradeSubscribe(market.symbol, true);
    });

    // markets_summary: bonus aggregate for slow-changing fields (volume_24h,
    // open_interest, funding_rate, etc.) that bbo doesn't carry.
    this.sendJsonRpcSubscription('subscribe', 'markets_summary');

    this.parent.log(`Subscribed to bbo + trades for ${markets.length} markets + markets_summary`, LogSeverity.Info);

    // Resubscribe to orderbooks that have active callbacks
    // This handles the case where listenOrderBook was called for symbols
    // that might not be in the store.markets list
    if (this.orderbookCallbacks.size > 0) {
      this.orderbookCallbacks.forEach((callbacks, symbol) => {
        if (callbacks.size > 0) {
          this.subscribeToOrderBook(symbol);
        }
      });
      this.parent.log(`Resubscribed to ${this.orderbookCallbacks.size} orderbooks`, LogSeverity.Info);
    }
  };

  subscribeToOrderBook = (symbol: string) => {
    if (this.isConnected) {
      // The deltas feed starts with a full-depth snapshot.
      const channel = getOrderBookChannel(symbol);
      this.sendJsonRpcSubscription('subscribe', channel);
      // this.parent.log(`Subscribing to orderbook: ${channel}`, LogSeverity.Debug);
    }
  };

  unsubscribeFromOrderBook = (symbol: string) => {
    if (this.isConnected) {
      const channel = getOrderBookChannel(symbol);
      this.sendJsonRpcSubscription('unsubscribe', channel);
      // this.parent.log(`Unsubscribing from orderbook: ${channel}`, LogSeverity.Debug);
    }
  };

  // trades.{symbol} is broadcast-subscribed for every market in subscribeToChannels,
  // but active OHLCV listeners also ask for their symbol explicitly. That gives
  // chart resubscribe/liveness paths a way to repair a missed or failed trade
  // channel without tearing down the whole public socket.
  subscribeToTrades = (symbol: string) => {
    const currentCount = this.tradeSubscriptionCounts.get(symbol) || 0;
    this.tradeSubscriptionCounts.set(symbol, currentCount + 1);
    this.sendTradeSubscribe(symbol);
  };

  unsubscribeFromTrades = (symbol: string) => {
    const currentCount = this.tradeSubscriptionCounts.get(symbol) || 0;
    if (currentCount <= 1) {
      this.tradeSubscriptionCounts.delete(symbol);
      return;
    }
    this.tradeSubscriptionCounts.set(symbol, currentCount - 1);
  };

  listenOrderBook = (symbol: string, callback: (orderbook: OrderBook) => void) => {
    // Get or create callback set for this symbol
    if (!this.orderbookCallbacks.has(symbol)) {
      this.orderbookCallbacks.set(symbol, new Set());
    }

    const callbacks = this.orderbookCallbacks.get(symbol)!;
    callbacks.add(callback);

    // Subscribe to orderbook if this is the first listener
    if (callbacks.size === 1) {
      this.subscribeToOrderBook(symbol);

      // Asynchronously set max slippage to 20% when listening to orderbook
      if (!this.parent.isPublicDataOnly && this.parent.jwtManager) {
        this.parent.setMaxSlippage('0.2', symbol).catch((err) => {
          this.parent.log(`Failed to set max slippage for ${symbol}: ${err}`, LogSeverity.Warning);
        });
      }
    }

    // Return unsubscribe function
    return () => {
      callbacks.delete(callback);

      // Unsubscribe from orderbook if no more listeners
      if (callbacks.size === 0) {
        this.orderbookCallbacks.delete(symbol);
        this.orderbooks.get(symbol)?.emit.cancel();
        this.orderbooks.delete(symbol);
        this.unsubscribeFromOrderBook(symbol);
        this.cleanupMessageHandlerStats(`order_book.${symbol}`);
      }
    };
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // Paradex doesn't support OHLCV via WebSocket natively,
    // so we use a hybrid approach: poll REST API + update from trades feed

    const pollKey = `${opts.symbol}:${opts.interval}`;

    // Calculate interval in milliseconds for trade updates
    const intervalMs = (timeframeToSeconds[opts.interval] || 60) * 1000;

    // Store callback and interval for trade-based updates. Multiple chart
    // listeners can share a symbol:interval; one cleanup must not tear down
    // another active listener.
    const callbacks = this.ohlcvCallbacks.get(pollKey);
    if (callbacks) {
      callbacks.add(callback);
      this.subscribeToTrades(opts.symbol);

      return () => {
        const latestCallbacks = this.ohlcvCallbacks.get(pollKey);
        latestCallbacks?.delete(callback);
        this.unsubscribeFromTrades(opts.symbol);
        if (latestCallbacks && latestCallbacks.size > 0) {
          return;
        }
        this.cleanupOhlcvSubscription(pollKey);
      };
    }

    this.ohlcvCallbacks.set(pollKey, new Set([callback]));
    this.ohlcvIntervals.set(pollKey, intervalMs);

    // Subscribe to symbol trades while this OHLCV listener is active.
    this.subscribeToTrades(opts.symbol);

    // Mark polling as active
    const subscriptionVersion = (this.ohlcvSubscriptionVersions.get(pollKey) || 0) + 1;
    this.ohlcvSubscriptionVersions.set(pollKey, subscriptionVersion);
    this.ohlcvPollingActive.set(pollKey, true);

    // Asynchronously set max slippage to 20% when listening to OHLCV
    if (!this.parent.isPublicDataOnly && this.parent.jwtManager) {
      this.parent.setMaxSlippage('0.2', opts.symbol).catch((err) => {
        this.parent.log(`Failed to set max slippage for ${opts.symbol}: ${err}`, LogSeverity.Warning);
      });
    }

    // Recursive polling function using setTimeout
    const pollCandle = async () => {
      // Check if polling is still active
      if (!this.ohlcvPollingActive.get(pollKey)) {
        return;
      }

      try {
        const now = Date.now();

        // Calculate how far back to fetch based on interval

        // Fetch last 2 candles (go back 3x interval to be safe)
        const from = now - intervalMs * 3;

        const revision = this.ohlcvRevisions.get(pollKey) || 0;
        const candles = await this.parent.fetchOHLCV({
          symbol: opts.symbol,
          interval: opts.interval,
          from: from,
          to: now,
        });

        if (
          !this.ohlcvPollingActive.get(pollKey) ||
          this.ohlcvSubscriptionVersions.get(pollKey) !== subscriptionVersion
        ) {
          return;
        }

        if (candles.length > 0 && revision === (this.ohlcvRevisions.get(pollKey) || 0)) {
          const latestCandle = candles[candles.length - 1];
          const previousCandle = this.lastCandleCache.get(pollKey);
          // REST can lag a trade that was already present before this poll began.
          const advances = !previousCandle || latestCandle.timestamp > previousCandle.timestamp || (
            latestCandle.timestamp === previousCandle.timestamp && latestCandle.volume >= previousCandle.volume
          );

          // Only emit if the candle has changed
          if (
            advances && (
              !previousCandle ||
              previousCandle.timestamp !== latestCandle.timestamp ||
              previousCandle.close !== latestCandle.close ||
              previousCandle.volume !== latestCandle.volume
            )
          ) {
            this.lastCandleCache.set(pollKey, latestCandle);
            this.ohlcvCallbacks.get(pollKey)?.forEach((activeCallback) => activeCallback(latestCandle));
          }
        }
        this.maybeRecoverOhlcvTradeStream(opts, pollKey);
      } catch {
        // this.parent.log(`Error polling OHLCV for ${opts.symbol}: ${err}`, LogSeverity.Warning);
      }

      // Schedule next poll only if still active (poll every 3 seconds)
      if (
        this.ohlcvPollingActive.get(pollKey) &&
        this.ohlcvSubscriptionVersions.get(pollKey) === subscriptionVersion
      ) {
        const timeoutId = setTimeout(pollCandle, OHLCV_POLL_INTERVAL_MS);
        this.ohlcvPollingTimeouts.set(pollKey, timeoutId);
      }
    };

    // Start polling immediately
    pollCandle();

    // Return cleanup function
    return () => {
      const latestCallbacks = this.ohlcvCallbacks.get(pollKey);
      latestCallbacks?.delete(callback);
      this.unsubscribeFromTrades(opts.symbol);
      if (latestCallbacks && latestCallbacks.size > 0) {
        return;
      }

      this.cleanupOhlcvSubscription(pollKey);
    };
  };

  private cleanupOhlcvSubscription = (pollKey: string) => {
    this.ohlcvRevisions.delete(pollKey);
    // Mark as inactive and bump the generation so an in-flight REST poll cannot
    // schedule itself again after a new listener has replaced this one.
    this.ohlcvPollingActive.set(pollKey, false);
    this.ohlcvSubscriptionVersions.set(pollKey, (this.ohlcvSubscriptionVersions.get(pollKey) || 0) + 1);

    if (this.ohlcvPollingTimeouts.has(pollKey)) {
      clearTimeout(this.ohlcvPollingTimeouts.get(pollKey)!);
      this.ohlcvPollingTimeouts.delete(pollKey);
    }

    this.ohlcvCallbacks.delete(pollKey);
    this.ohlcvIntervals.delete(pollKey);
    this.ohlcvLastTradeAt.delete(pollKey);
    this.ohlcvLastRecoveryAt.delete(pollKey);
    this.lastCandleCache.delete(pollKey);
  };
}
