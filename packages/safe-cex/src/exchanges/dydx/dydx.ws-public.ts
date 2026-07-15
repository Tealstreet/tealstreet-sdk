import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, LogSeverity, OHLCVOptions, OrderBook, OrderSide, Ticker, Writable } from '../../types';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { updateOrderBookByPrice } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { DydxExchange } from './dydx.exchange';
import {
  CandleResolution,
  DydxCandle,
  DydxOrderbookDeltaLevel,
  DydxOrderbookSnapshotLevel,
  DydxWsCandlesUpdate,
  DydxWsChannelBatchData,
  DydxWsChannelData,
  DydxWsMarketsUpdate,
  DydxWsOrderbookUpdate,
  DydxWsSubscribeMessage,
  DydxWsTradesUpdate,
  WS_CHANNELS,
  WSS_URLS,
} from './dydx.types';

type MessageHandlers = {
  [topic: string]: Array<(json: any) => void>;
};

type SubscribedTopics = {
  [id: string]: number;
};

type DydxOrderbookMessage = {
  data: DydxWsOrderbookUpdate;
  isSnapshot: boolean;
};

/**
 * dYdX v4 Public WebSocket client
 * Handles real-time public market data: tickers, orderbooks, trades, candles
 */
export class DydxPublicWebSocket extends BaseWebSocket<DydxExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};
  private subscriptions = new Set<string>();

  constructor(parent: DydxExchange) {
    super(parent);
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      const wsUrl = WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION'];
      this.ws = this.createWebsocket(wsUrl);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', () => {
        this.onClose();
      });
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.subscriptions.clear();
      this.subscribe();
      this.resubscribeActiveTopics();
      this.emitWsReconnected();
    }
  };

  subscribe = () => {
    // Subscribe to public channels only
    this.subscribeToMarkets();
    this.subscribeToBlockHeight();
    for (const market of this.parent.store.markets) {
      this.subscribeToTrades(market.id);
    }
  };

  private resubscribeActiveTopics = () => {
    for (const [topic, listeners] of Object.entries(this.topics)) {
      if (listeners <= 0) continue;

      const orderbookPrefix = `${WS_CHANNELS.ORDERBOOK}.`;
      if (topic.startsWith(orderbookPrefix)) {
        this.subscribeToOrderbook(topic.slice(orderbookPrefix.length));
        continue;
      }

      const candlesPrefix = `${WS_CHANNELS.CANDLES}.`;
      if (topic.startsWith(candlesPrefix)) {
        const [market, resolution] = topic.slice(candlesPrefix.length).split('.');
        if (market && resolution) {
          this.subscribeToCandles(market, resolution as CandleResolution);
        }
      }
    }
  };

  /**
   * Subscribe to all markets channel
   * Receives updates for all market data including prices and funding rates
   */
  subscribeToMarkets = () => {
    const channel = WS_CHANNELS.MARKETS;
    const subscriptionKey = channel;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to ${channel}`, LogSeverity.Info);
  };

  /**
   * Subscribe to orderbook updates for a specific market
   */
  subscribeToOrderbook = (market: string) => {
    const channel = WS_CHANNELS.ORDERBOOK;
    const subscriptionKey = `${channel}:${market}`;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
      id: market,
      batched: true,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to orderbook for ${market}`, LogSeverity.Info);
  };

  /**
   * Subscribe to trades for a specific market
   */
  subscribeToTrades = (market: string) => {
    const channel = WS_CHANNELS.TRADES;
    const subscriptionKey = `${channel}:${market}`;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
      id: market,
      batched: true,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to trades for ${market}`, LogSeverity.Info);
  };

  /**
   * Subscribe to candles for a specific market and resolution
   */
  subscribeToCandles = (market: string, resolution: CandleResolution) => {
    const channel = WS_CHANNELS.CANDLES;
    const id = `${market}/${resolution}`;
    const subscriptionKey = `${channel}:${id}`;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
      id,
      batched: true,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to candles for ${market} ${resolution}`, LogSeverity.Info);
  };

  /**
   * Subscribe to block height updates
   */
  subscribeToBlockHeight = () => {
    const channel = WS_CHANNELS.BLOCK_HEIGHT;
    const subscriptionKey = channel;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to ${channel}`, LogSeverity.Info);
  };

  /**
   * Unsubscribe from orderbook updates
   */
  unsubscribeFromOrderbook = (market: string) => {
    const channel = WS_CHANNELS.ORDERBOOK;
    const subscriptionKey = `${channel}:${market}`;

    if (!this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'unsubscribe',
      channel,
      id: market,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.delete(subscriptionKey);
    this.parent.log(`Unsubscribed from orderbook for ${market}`, LogSeverity.Info);
  };

  /**
   * Unsubscribe from trades
   */
  unsubscribeFromTrades = (market: string) => {
    const channel = WS_CHANNELS.TRADES;
    const subscriptionKey = `${channel}:${market}`;

    if (!this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'unsubscribe',
      channel,
      id: market,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.delete(subscriptionKey);
    this.parent.log(`Unsubscribed from trades for ${market}`, LogSeverity.Info);
  };

  /**
   * Unsubscribe from candles
   */
  unsubscribeFromCandles = (market: string, resolution: CandleResolution) => {
    const channel = WS_CHANNELS.CANDLES;
    const id = `${market}/${resolution}`;
    const subscriptionKey = `${channel}:${id}`;

    if (!this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'unsubscribe',
      channel,
      id,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.delete(subscriptionKey);
    this.parent.log(`Unsubscribed from candles for ${market} ${resolution}`, LogSeverity.Info);
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        const messageType = json.type;
        const channel = json.channel;

        // Record stats for non-orderbook channels (orderbook is too frequent)
        if (channel !== WS_CHANNELS.ORDERBOOK) {
          this.recordMessageHandlerStats(channel);
        }

        // Handle subscription responses
        if (messageType === 'subscribed') {
          this.parent.log(`Subscription confirmed for ${channel}`, LogSeverity.Info);
          if (channel === WS_CHANNELS.ORDERBOOK && json.id && json.contents) {
            this.handleOrderbookUpdate(json.id, json.contents as DydxWsOrderbookUpdate, true);
          }
          return;
        }

        if (messageType === 'unsubscribed') {
          this.parent.log(`Unsubscription confirmed for ${channel}`, LogSeverity.Info);
          return;
        }

        if (messageType === 'error') {
          this.parent.log(`WebSocket error: ${json.message || 'Unknown error'}`, LogSeverity.Error);
          return;
        }

        // Handle channel data
        if (messageType === 'channel_data') {
          this.handleChannelData(json as DydxWsChannelData);
        } else if (messageType === 'channel_batch_data') {
          this.handleChannelBatchData(json as DydxWsChannelBatchData);
        }
      } catch (e) {
        this.parent.log(`Error parsing WebSocket message: ${e}`, LogSeverity.Error);
      }
    }
  };

  /**
   * Handle individual channel data messages
   */
  private handleChannelData = (message: DydxWsChannelData) => {
    const { channel, contents, id } = message;

    switch (channel) {
      case WS_CHANNELS.BLOCK_HEIGHT:
        this.handleBlockHeight(contents);
        break;
      default:
        // Execute registered message handlers
        const handlerKey = id ? `${channel}.${id}` : channel;
        if (this.messageHandlers[handlerKey]) {
          for (const fn of this.messageHandlers[handlerKey]) {
            fn(contents);
          }
        }
    }
  };

  /**
   * Handle batched channel data messages
   */
  private handleChannelBatchData = (message: DydxWsChannelBatchData) => {
    const { channel, contents, id } = message;

    if (!Array.isArray(contents)) {
      this.parent.log(`Expected array for batch data in channel ${channel}`, LogSeverity.Warning);
      return;
    }

    for (const item of contents) {
      switch (channel) {
        case WS_CHANNELS.MARKETS:
          this.handleMarketsUpdate(item as DydxWsMarketsUpdate);
          break;
        case WS_CHANNELS.TRADES:
          this.handleTradesUpdate(id || '', item as DydxWsTradesUpdate);
          break;
        case WS_CHANNELS.ORDERBOOK:
          this.handleOrderbookUpdate(id || '', item as DydxWsOrderbookUpdate, false);
          break;
        case WS_CHANNELS.CANDLES:
          this.handleCandlesUpdate(item as DydxWsCandlesUpdate);
          break;
        default:
          const handlerKey = id ? `${channel}.${id}` : channel;
          if (this.messageHandlers[handlerKey]) {
            for (const fn of this.messageHandlers[handlerKey]) {
              fn(item);
            }
          }
      }
    }
  };

  /**
   * Handle markets channel updates
   * Updates ticker data for all markets
   */
  private handleMarketsUpdate = (data: DydxWsMarketsUpdate) => {
    if (!data.trading) return;

    // O(1) hashmap lookup. v4_markets batch frames carry updates for many
    // tickers per frame; the prior tickers.find was N*M per frame.
    const tickersById = this.parent.store.tickersById;
    for (const tickerSymbol in data.trading) {
      const marketData = data.trading[tickerSymbol];
      const ticker = tickersById[tickerSymbol];

      if (ticker) {
        const update: Partial<Writable<Ticker>> = {};

        if (marketData.oraclePrice) {
          update.index = parseFloat(marketData.oraclePrice);
        }
        if (marketData.priceChange24H) {
          update.percentage = parseFloat(marketData.priceChange24H);
        }
        if (marketData.volume24H) {
          update.volume = parseFloat(marketData.volume24H);
        }
        if (marketData.nextFundingRate) {
          update.fundingRate = parseFloat(marketData.nextFundingRate);
        }
        if (marketData.openInterest) {
          update.openInterest = parseFloat(marketData.openInterest);
        }

        this.parent.store.updateTicker(ticker, update);
      }
    }
  };

  /**
   * Handle trades channel updates
   */
  private handleTradesUpdate = (marketId: string, data: DydxWsTradesUpdate) => {
    if (!marketId || !data.trades || !Array.isArray(data.trades)) return;

    const market = this.parent.store.marketsById[marketId];
    const ticker = this.parent.store.tickersById[marketId];
    if (!market) return;

    for (const trade of data.trades) {
      const price = parseFloat(trade.price);
      const amount = parseFloat(trade.size);
      if (!Number.isFinite(price) || !Number.isFinite(amount)) continue;

      if (ticker) {
        this.parent.store.updateTicker(ticker, { last: price });
      }

      this.parent.emitter.emit('fill', {
        id: trade.id,
        side: trade.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
        symbol: market.symbol,
        price,
        amount,
      });
    }
  };

  /**
   * Handle orderbook channel updates
   */
  private handleOrderbookUpdate = (marketId: string, data: DydxWsOrderbookUpdate, isSnapshot: boolean) => {
    const handlerKey = `${WS_CHANNELS.ORDERBOOK}.${marketId}`;

    if (this.messageHandlers[handlerKey]) {
      for (const fn of this.messageHandlers[handlerKey]) {
        fn({ data, isSnapshot } satisfies DydxOrderbookMessage);
      }
    }
  };

  /**
   * Handle candles channel updates
   */
  private handleCandlesUpdate = (data: DydxWsCandlesUpdate) => {
    if (!data.candles || !Array.isArray(data.candles)) return;

    for (const candle of data.candles) {
      const handlerKey = `${WS_CHANNELS.CANDLES}.${candle.ticker}.${candle.resolution}`;

      if (this.messageHandlers[handlerKey]) {
        for (const fn of this.messageHandlers[handlerKey]) {
          fn(candle);
        }
      }
    }
  };

  /**
   * Handle block height updates
   */
  private handleBlockHeight = (data: any) => {
    if (data.height) {
      this.parent.log(`Block height: ${data.height}`, LogSeverity.Debug);
    }
  };

  /**
   * Listen to OHLCV candles for a specific market and interval
   */
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) {
      this.parent.log(`Market with symbol ${opts.symbol} not found`, LogSeverity.Error);
      return () => {};
    }

    const marketId = market.id;
    const resolution = this.mapIntervalToResolution(opts.interval);
    const topic = `${WS_CHANNELS.CANDLES}.${marketId}.${resolution}`;
    const handler = topic;

    const parser = (data: DydxCandle) => {
      if (data.ticker === marketId && data.resolution === resolution) {
        this.recordMessageHandlerStats(topic);
        callback({
          timestamp: new Date(data.startedAt).getTime(),
          open: parseFloat(data.open),
          high: parseFloat(data.high),
          low: parseFloat(data.low),
          close: parseFloat(data.close),
          volume: parseFloat(data.baseTokenVolume),
        });
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.subscribeToCandles(marketId, resolution);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;
      if (this.topics[topic] <= 0) delete this.topics[topic];

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        this.unsubscribeFromCandles(marketId, resolution);
      }
    };
  };

  /**
   * Listen to orderbook updates for a specific market
   */
  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === symbol);
    if (!market) {
      this.parent.log(`Market with symbol ${symbol} not found`, LogSeverity.Error);
      return () => {};
    }

    const marketId = market.id;
    const topic = `${WS_CHANNELS.ORDERBOOK}.${marketId}`;
    const handler = topic;
    const innerId = nanoid();

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const orderBook: OrderBook = { bids: [], asks: [] };

    const parseSnapshotLevel = (level: DydxOrderbookSnapshotLevel) => {
      if (!level || typeof level.price !== 'string' || typeof level.size !== 'string') return undefined;
      const price = Number(level.price);
      const amount = Number(level.size);
      if (!Number.isFinite(price) || price <= 0 || !Number.isFinite(amount) || amount <= 0) return undefined;
      return { price, amount, total: 0 };
    };

    const applyDelta = (side: 'bids' | 'asks', level: DydxOrderbookDeltaLevel) => {
      if (!Array.isArray(level) || level.length !== 2) return;
      const [rawPrice, rawAmount] = level;
      if (typeof rawPrice !== 'string' || typeof rawAmount !== 'string') return;
      const price = Number(rawPrice);
      const amount = Number(rawAmount);
      if (!Number.isFinite(price) || price <= 0 || !Number.isFinite(amount) || amount < 0) return;
      updateOrderBookByPrice(orderBook, side, price, amount);
    };

    const parser = ({ data, isSnapshot }: DydxOrderbookMessage) => {
      if (!data || (!data.bids && !data.asks)) return;

      if (isSnapshot) {
        orderBook.bids = (data.bids || [])
          .map((level) => parseSnapshotLevel(level as DydxOrderbookSnapshotLevel))
          .filter((level): level is OrderBook['bids'][number] => level !== undefined);
        orderBook.asks = (data.asks || [])
          .map((level) => parseSnapshotLevel(level as DydxOrderbookSnapshotLevel))
          .filter((level): level is OrderBook['asks'][number] => level !== undefined);
        this.orderBookInnerState[innerId].isSnapshotLoaded = true;
      } else {
        if (!this.orderBookInnerState[innerId].isSnapshotLoaded) return;
        for (const level of data.bids || []) applyDelta('bids', level as DydxOrderbookDeltaLevel);
        for (const level of data.asks || []) applyDelta('asks', level as DydxOrderbookDeltaLevel);
      }

      if (orderBook.asks.length !== 0 || orderBook.bids.length !== 0) {
        calcOrderBookTotal(orderBook);
        callback({
          bids: orderBook.bids.map((level) => ({ ...level })),
          asks: orderBook.asks.map((level) => ({ ...level })),
        });
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.subscribeToOrderbook(marketId);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      delete this.orderBookInnerState[innerId];
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;
      if (this.topics[topic] <= 0) delete this.topics[topic];

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        this.unsubscribeFromOrderbook(marketId);
      }
    };
  };

  /**
   * Map interval string to dYdX CandleResolution
   */
  private mapIntervalToResolution(interval: string): CandleResolution {
    const mapping: Record<string, CandleResolution> = {
      '1m': CandleResolution.ONE_MIN,
      '5m': CandleResolution.FIVE_MINS,
      '15m': CandleResolution.FIFTEEN_MINS,
      '30m': CandleResolution.THIRTY_MINS,
      '1h': CandleResolution.ONE_HOUR,
      '4h': CandleResolution.FOUR_HOURS,
      '1d': CandleResolution.ONE_DAY,
    };

    return mapping[interval] || CandleResolution.ONE_MIN;
  }

  /**
   * dYdX doesn't require explicit ping, connection is maintained by server
   */
  ping = (): void => {
    // No-op: dYdX maintains the connection automatically
  };
}
